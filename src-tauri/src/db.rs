use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    ai::AiSettings, collector::WindowSample, ActivityPoint, AiProviderSettingsMasked,
    AiSettingsInput, AiSettingsMasked, AppPreferences, AppUsage, ChatMessage, DailyReport,
    ReportContext, Session,
};

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let conn = Connection::open(path)?;
        let database = Self { conn };
        database.init()?;
        Ok(database)
    }

    #[cfg(test)]
    pub fn memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        let database = Self { conn };
        database.init()?;
        Ok(database)
    }

    fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              started_at TEXT NOT NULL,
              ended_at TEXT,
              status TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS window_samples (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id INTEGER NOT NULL,
              sampled_at TEXT NOT NULL,
              app_name TEXT NOT NULL,
              window_title TEXT NOT NULL,
              exe_path TEXT
            );

            CREATE TABLE IF NOT EXISTS app_usage (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id INTEGER NOT NULL,
              app_name TEXT NOT NULL,
              exe_path TEXT,
              seconds INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS activity_events (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id INTEGER,
              event_type TEXT NOT NULL,
              count INTEGER NOT NULL,
              recorded_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pomodoro_events (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              event_type TEXT NOT NULL,
              recorded_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS daily_reports (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id INTEGER NOT NULL,
              started_at TEXT NOT NULL,
              ended_at TEXT NOT NULL,
              total_seconds INTEGER NOT NULL,
              focus_score INTEGER NOT NULL,
              app_usage_json TEXT NOT NULL DEFAULT '[]',
              activity_json TEXT NOT NULL DEFAULT '[]',
              pomodoro_completed INTEGER NOT NULL DEFAULT 0,
              ai_summary TEXT
            );

            CREATE TABLE IF NOT EXISTS ai_settings (
              provider TEXT NOT NULL DEFAULT 'custom',
              base_url TEXT NOT NULL,
              api_key TEXT NOT NULL,
              model TEXT NOT NULL,
              PRIMARY KEY(provider)
            );

            CREATE TABLE IF NOT EXISTS chat_messages (
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              report_id INTEGER NOT NULL,
              role TEXT NOT NULL,
              content TEXT NOT NULL,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS app_preferences (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            ",
        )?;
        self.ensure_ai_settings_provider_table()?;
        Ok(())
    }

    fn create_provider_ai_settings_table(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS ai_settings (
              provider TEXT NOT NULL DEFAULT 'custom',
              base_url TEXT NOT NULL,
              api_key TEXT NOT NULL,
              model TEXT NOT NULL,
              PRIMARY KEY(provider)
            );
            ",
        )
    }

    fn ensure_ai_settings_provider_table(&self) -> rusqlite::Result<()> {
        let table_sql = self
            .conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'ai_settings'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if table_sql
            .as_deref()
            .map(|sql| {
                sql.contains("PRIMARY KEY(provider)") || sql.contains("PRIMARY KEY (provider)")
            })
            .unwrap_or(false)
        {
            return Ok(());
        }

        let mut statement = self.conn.prepare("PRAGMA table_info(ai_settings)")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let has_provider = columns.iter().any(|column| column == "provider");
        let legacy = self
            .conn
            .query_row(
                "SELECT base_url, api_key, model FROM ai_settings LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let legacy_provider = if has_provider {
            self.conn
                .query_row("SELECT provider FROM ai_settings LIMIT 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?
                .unwrap_or_else(|| "custom".into())
        } else {
            "custom".into()
        };

        self.conn
            .execute_batch("ALTER TABLE ai_settings RENAME TO ai_settings_legacy;")?;
        self.create_provider_ai_settings_table()?;
        let active_provider = if legacy_provider == "deepseek" || legacy_provider == "custom" {
            legacy_provider
        } else {
            "deepseek".into()
        };
        if let Some((base_url, api_key, model)) = legacy {
            let provider = if active_provider == "deepseek" || active_provider == "custom" {
                active_provider.as_str()
            } else {
                "custom"
            };
            self.conn.execute(
                "INSERT OR REPLACE INTO ai_settings (provider, base_url, api_key, model)
                 VALUES (?1, ?2, ?3, ?4)",
                params![provider, base_url, api_key, model],
            )?;
        }
        self.set_preference_value("active_ai_provider", &active_provider)?;
        self.conn
            .execute_batch("DROP TABLE IF EXISTS ai_settings_legacy;")?;
        Ok(())
    }

    #[cfg(test)]
    pub fn table_exists(&self, table_name: &str) -> rusqlite::Result<bool> {
        self.conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table_name],
                |_| Ok(true),
            )
            .optional()
            .map(|value| value.unwrap_or(false))
    }

    pub fn start_session(&self) -> rusqlite::Result<Session> {
        let started_at = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO sessions (started_at, status) VALUES (?1, 'studying')",
            params![started_at],
        )?;
        self.get_session(self.conn.last_insert_rowid())
    }

    pub fn stop_session(&self, session_id: i64) -> rusqlite::Result<Session> {
        let ended_at = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1, status = 'ended' WHERE id = ?2",
            params![ended_at, session_id],
        )?;
        self.get_session(session_id)
    }

    pub fn close_stale_studying_sessions(&self) -> rusqlite::Result<usize> {
        self.conn.execute(
            "UPDATE sessions
             SET ended_at = started_at, status = 'ended'
             WHERE status = 'studying'",
            [],
        )
    }

    #[cfg(test)]
    pub fn add_session_with_times(
        &self,
        started_at: &str,
        ended_at: Option<&str>,
        status: &str,
    ) -> rusqlite::Result<Session> {
        self.conn.execute(
            "INSERT INTO sessions (started_at, ended_at, status) VALUES (?1, ?2, ?3)",
            params![started_at, ended_at, status],
        )?;
        self.get_session(self.conn.last_insert_rowid())
    }

    pub fn add_window_sample(
        &self,
        session_id: i64,
        sample: &WindowSample,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO window_samples
             (session_id, sampled_at, app_name, window_title, exe_path)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session_id,
                sample.sampled_at.as_str(),
                sample.app_name.as_str(),
                sample.window_title.as_str(),
                sample.exe_path.as_deref()
            ],
        )?;
        Ok(())
    }

    pub fn latest_window_sample(&self) -> rusqlite::Result<Option<WindowSample>> {
        self.conn
            .query_row(
                "SELECT app_name, window_title, exe_path, sampled_at
                 FROM window_samples
                 ORDER BY id DESC
                 LIMIT 1",
                [],
                |row| {
                    Ok(WindowSample {
                        app_name: row.get(0)?,
                        window_title: row.get(1)?,
                        exe_path: row.get(2)?,
                        sampled_at: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    pub fn aggregate_app_usage(&self, session_id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM app_usage WHERE session_id = ?1",
            params![session_id],
        )?;
        self.conn.execute(
            "INSERT INTO app_usage (session_id, app_name, exe_path, seconds)
             SELECT session_id, app_name, exe_path, COUNT(*) AS seconds
             FROM window_samples
             WHERE session_id = ?1
             GROUP BY session_id, app_name, exe_path",
            params![session_id],
        )?;
        Ok(())
    }

    pub fn app_usage_from_samples_for_session(
        &self,
        session_id: i64,
    ) -> rusqlite::Result<Vec<AppUsage>> {
        let mut statement = self.conn.prepare(
            "SELECT app_name, exe_path, COUNT(*) AS seconds
             FROM window_samples
             WHERE session_id = ?1
             GROUP BY app_name, exe_path
             ORDER BY seconds DESC, app_name ASC",
        )?;
        let usage = statement
            .query_map(params![session_id], |row| {
                Ok(AppUsage {
                    app_name: row.get(0)?,
                    exe_path: row.get(1)?,
                    seconds: row.get(2)?,
                })
            })?
            .collect();
        usage
    }

    pub fn app_usage_for_session(&self, session_id: i64) -> rusqlite::Result<Vec<AppUsage>> {
        let mut statement = self.conn.prepare(
            "SELECT app_name, exe_path, seconds
             FROM app_usage
             WHERE session_id = ?1
             ORDER BY seconds DESC, app_name ASC",
        )?;
        let usage = statement
            .query_map(params![session_id], |row| {
                Ok(AppUsage {
                    app_name: row.get(0)?,
                    exe_path: row.get(1)?,
                    seconds: row.get(2)?,
                })
            })?
            .collect();
        usage
    }

    pub fn get_session(&self, session_id: i64) -> rusqlite::Result<Session> {
        self.conn.query_row(
            "SELECT id, started_at, ended_at, status FROM sessions WHERE id = ?1",
            params![session_id],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    started_at: row.get(1)?,
                    ended_at: row.get(2)?,
                    status: row.get(3)?,
                })
            },
        )
    }

    pub fn latest_report_id(&self) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM daily_reports ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn get_report_context(&self, report_id: i64) -> rusqlite::Result<ReportContext> {
        self.conn.query_row(
            "SELECT id, session_id, started_at, ended_at, total_seconds, focus_score,
                    app_usage_json, activity_json, pomodoro_completed, ai_summary
             FROM daily_reports
             WHERE id = ?1",
            params![report_id],
            |row| {
                Ok(ReportContext {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: row.get(2)?,
                    ended_at: row.get(3)?,
                    total_seconds: row.get(4)?,
                    focus_score: row.get(5)?,
                    app_usage_json: row.get(6)?,
                    activity_json: row.get(7)?,
                    pomodoro_completed: row.get(8)?,
                    ai_summary: row.get(9)?,
                })
            },
        )
    }

    pub fn create_daily_report(
        &self,
        session: &Session,
        total_seconds: i64,
        focus_score: i64,
        app_usage_json: &str,
        activity_json: &str,
        pomodoro_completed: i64,
        ai_summary: Option<&str>,
    ) -> rusqlite::Result<i64> {
        let ended_at = session.ended_at.as_deref().unwrap_or(&session.started_at);
        self.conn.execute(
            "INSERT INTO daily_reports
             (session_id, started_at, ended_at, total_seconds, focus_score,
              app_usage_json, activity_json, pomodoro_completed, ai_summary)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                session.id,
                session.started_at.as_str(),
                ended_at,
                total_seconds,
                focus_score,
                app_usage_json,
                activity_json,
                pomodoro_completed,
                ai_summary
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_report_summary(&self, report_id: i64, summary: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "UPDATE daily_reports SET ai_summary = ?1 WHERE id = ?2",
            params![summary, report_id],
        )?;
        Ok(())
    }

    pub fn recent_daily_reports(&self, limit: i64) -> rusqlite::Result<Vec<DailyReport>> {
        let limit = limit.clamp(1, 100);
        let mut statement = self.conn.prepare(
            "SELECT id, session_id, started_at, ended_at, total_seconds, focus_score,
                    app_usage_json, activity_json, pomodoro_completed, ai_summary
             FROM daily_reports
             ORDER BY ended_at DESC, id DESC
             LIMIT ?1",
        )?;
        let reports = statement
            .query_map(params![limit], |row| {
                let app_usage_json: String = row.get(6)?;
                let activity_json: String = row.get(7)?;
                Ok(DailyReport {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    started_at: row.get(2)?,
                    ended_at: row.get(3)?,
                    total_seconds: row.get(4)?,
                    focus_score: row.get(5)?,
                    app_usage: serde_json::from_str(&app_usage_json).unwrap_or_default(),
                    activity: serde_json::from_str(&activity_json).unwrap_or_default(),
                    pomodoro_completed: row.get(8)?,
                    ai_summary: row.get(9)?,
                })
            })?
            .collect();
        reports
    }

    pub fn delete_daily_report(&self, report_id: i64) -> rusqlite::Result<()> {
        self.conn.execute(
            "DELETE FROM chat_messages WHERE report_id = ?1",
            params![report_id],
        )?;
        self.conn.execute(
            "DELETE FROM daily_reports WHERE id = ?1",
            params![report_id],
        )?;
        Ok(())
    }

    pub fn clear_local_data(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "
            DELETE FROM chat_messages;
            DELETE FROM daily_reports;
            DELETE FROM app_usage;
            DELETE FROM window_samples;
            DELETE FROM activity_events;
            DELETE FROM pomodoro_events;
            DELETE FROM sessions;
            ",
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn chat_message_count_for_report(&self, report_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM chat_messages WHERE report_id = ?1",
            params![report_id],
            |row| row.get(0),
        )
    }

    #[cfg(test)]
    pub fn report_count(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM daily_reports", [], |row| row.get(0))
    }

    #[cfg(test)]
    pub fn report_total_seconds(&self, report_id: i64) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT total_seconds FROM daily_reports WHERE id = ?1",
            params![report_id],
            |row| row.get(0),
        )
    }

    #[cfg(test)]
    pub fn report_app_usage_json(&self, report_id: i64) -> rusqlite::Result<String> {
        self.conn.query_row(
            "SELECT app_usage_json FROM daily_reports WHERE id = ?1",
            params![report_id],
            |row| row.get(0),
        )
    }

    pub fn today_study_seconds(&self, now: DateTime<Utc>) -> rusqlite::Result<i64> {
        let mut statement = self
            .conn
            .prepare("SELECT started_at, ended_at, status FROM sessions")?;
        let mut total = 0;
        let today = now.date_naive();

        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows {
            let (started_at, ended_at, status) = row?;
            let Ok(started) = DateTime::parse_from_rfc3339(&started_at) else {
                continue;
            };
            let started = started.with_timezone(&Utc);
            if started.date_naive() != today {
                continue;
            }

            let ended = match ended_at {
                Some(value) => DateTime::parse_from_rfc3339(&value)
                    .map(|value| value.with_timezone(&Utc))
                    .unwrap_or(started),
                None if status == "studying" => now,
                None => started,
            };
            total += (ended - started).num_seconds().max(0);
        }

        Ok(total)
    }

    pub fn app_usage_from_samples_since(&self, since: &str) -> rusqlite::Result<Vec<AppUsage>> {
        let mut statement = self.conn.prepare(
            "SELECT app_name, exe_path, COUNT(*) AS seconds
             FROM window_samples
             WHERE sampled_at >= ?1
             GROUP BY app_name, exe_path
             ORDER BY seconds DESC, app_name ASC",
        )?;
        let usage = statement
            .query_map(params![since], |row| {
                Ok(AppUsage {
                    app_name: row.get(0)?,
                    exe_path: row.get(1)?,
                    seconds: row.get(2)?,
                })
            })?
            .collect();
        usage
    }

    pub fn add_activity_event(
        &self,
        session_id: i64,
        event_type: &str,
        count: i64,
    ) -> rusqlite::Result<()> {
        if event_type != "keyboard" && event_type != "mouse" {
            return Err(rusqlite::Error::InvalidParameterName(format!(
                "unsupported activity event type: {event_type}"
            )));
        }
        if count <= 0 {
            return Ok(());
        }

        self.conn.execute(
            "INSERT INTO activity_events (session_id, event_type, count, recorded_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, event_type, count, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn activity_totals_for_session(&self, session_id: i64) -> rusqlite::Result<(i64, i64)> {
        let keyboard = self.activity_total_for_type(session_id, "keyboard")?;
        let mouse = self.activity_total_for_type(session_id, "mouse")?;
        Ok((keyboard, mouse))
    }

    fn activity_total_for_type(&self, session_id: i64, event_type: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COALESCE(SUM(count), 0)
             FROM activity_events
             WHERE session_id = ?1 AND event_type = ?2",
            params![session_id, event_type],
            |row| row.get(0),
        )
    }

    pub fn activity_points_for_session(
        &self,
        session_id: i64,
    ) -> rusqlite::Result<Vec<ActivityPoint>> {
        let mut statement = self.conn.prepare(
            "SELECT recorded_at,
                    SUM(CASE WHEN event_type = 'keyboard' THEN count ELSE 0 END) AS keyboard,
                    SUM(CASE WHEN event_type = 'mouse' THEN count ELSE 0 END) AS mouse
             FROM activity_events
             WHERE session_id = ?1
             GROUP BY recorded_at
             ORDER BY recorded_at ASC",
        )?;
        let points = statement
            .query_map(params![session_id], |row| {
                let recorded_at: String = row.get(0)?;
                Ok(ActivityPoint {
                    label: activity_label(&recorded_at),
                    keyboard: row.get(1)?,
                    mouse: row.get(2)?,
                })
            })?
            .collect();
        points
    }

    #[cfg(test)]
    pub fn latest_studying_session_id(&self) -> rusqlite::Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM sessions WHERE status = 'studying' ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
    }

    pub fn save_ai_settings(&self, settings: &AiSettingsInput) -> rusqlite::Result<()> {
        let provider = normalize_saved_provider(settings.provider.as_deref());
        self.conn.execute(
            "INSERT INTO ai_settings (provider, base_url, api_key, model)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(provider) DO UPDATE
             SET base_url = excluded.base_url,
                 api_key = excluded.api_key,
                 model = excluded.model",
            params![
                provider,
                settings.base_url.trim(),
                settings.api_key.trim(),
                settings.model.trim()
            ],
        )?;
        self.set_preference_value("active_ai_provider", provider)?;
        Ok(())
    }

    pub fn get_ai_settings_masked(&self) -> rusqlite::Result<AiSettingsMasked> {
        Ok(AiSettingsMasked {
            active_provider: self.active_ai_provider()?,
            providers: vec![
                self.masked_ai_provider("deepseek")?,
                self.masked_ai_provider("custom")?,
            ],
        })
    }

    pub fn get_ai_settings(&self) -> rusqlite::Result<Option<AiSettings>> {
        self.get_ai_settings_for_provider(&self.active_ai_provider()?)
    }

    pub fn get_ai_settings_for_provider(
        &self,
        provider: &str,
    ) -> rusqlite::Result<Option<AiSettings>> {
        self.conn
            .query_row(
                "SELECT base_url, api_key, model FROM ai_settings WHERE provider = ?1",
                params![provider],
                |row| {
                    Ok(AiSettings {
                        base_url: row.get(0)?,
                        api_key: row.get(1)?,
                        model: row.get(2)?,
                    })
                },
            )
            .optional()
    }

    fn active_ai_provider(&self) -> rusqlite::Result<String> {
        Ok(self
            .preference_value("active_ai_provider")?
            .filter(|provider| provider == "deepseek" || provider == "custom")
            .unwrap_or_else(|| "deepseek".into()))
    }

    fn masked_ai_provider(&self, provider: &str) -> rusqlite::Result<AiProviderSettingsMasked> {
        let saved = self.get_ai_settings_for_provider(provider)?;
        let (default_base_url, default_model, available_models, base_url_editable) =
            if provider == "deepseek" {
                (
                    "https://api.deepseek.com",
                    "deepseek-v4-pro",
                    vec!["deepseek-v4-pro".into(), "deepseek-v4-flash".into()],
                    false,
                )
            } else {
                ("", "", Vec::new(), true)
            };

        Ok(AiProviderSettingsMasked {
            provider: provider.into(),
            base_url: saved
                .as_ref()
                .and_then(|settings| non_empty_or_none(&settings.base_url))
                .unwrap_or_else(|| default_base_url.into()),
            model: saved
                .as_ref()
                .and_then(|settings| non_empty_or_none(&settings.model))
                .unwrap_or_else(|| default_model.into()),
            api_key_masked: saved
                .as_ref()
                .map(|settings| mask_api_key(&settings.api_key))
                .unwrap_or_default(),
            configured: saved
                .as_ref()
                .map(|settings| !settings.api_key.trim().is_empty())
                .unwrap_or(false),
            available_models,
            base_url_editable,
            api_key_required: true,
        })
    }

    pub fn add_chat_message(
        &self,
        report_id: i64,
        role: &str,
        content: &str,
    ) -> rusqlite::Result<ChatMessage> {
        let created_at = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO chat_messages (report_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![report_id, role, content, created_at],
        )?;
        Ok(ChatMessage {
            id: self.conn.last_insert_rowid(),
            report_id,
            role: role.into(),
            content: content.into(),
            created_at,
        })
    }

    pub fn chat_messages_for_report(&self, report_id: i64) -> rusqlite::Result<Vec<ChatMessage>> {
        let mut statement = self.conn.prepare(
            "SELECT id, report_id, role, content, created_at
             FROM chat_messages
             WHERE report_id = ?1
             ORDER BY id ASC",
        )?;
        let messages = statement
            .query_map(params![report_id], |row| {
                Ok(ChatMessage {
                    id: row.get(0)?,
                    report_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect();
        messages
    }

    pub fn add_pomodoro_event(&self, event_type: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO pomodoro_events (event_type, recorded_at) VALUES (?1, ?2)",
            params![event_type, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_app_preferences(&self) -> rusqlite::Result<AppPreferences> {
        let privacy_notice_accepted = self
            .preference_value("privacy_notice_accepted")?
            .map(|value| value == "true")
            .unwrap_or(false);
        let default_pomodoro_minutes = self
            .preference_value("default_pomodoro_minutes")?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(25)
            .clamp(1, 180);
        let ai_summary_tone = self
            .preference_value("ai_summary_tone")?
            .unwrap_or_else(|| "witty".into());
        let activity_capture_enabled = self
            .preference_value("activity_capture_enabled")?
            .map(|value| value == "true")
            .unwrap_or(true);

        Ok(AppPreferences {
            privacy_notice_accepted,
            default_pomodoro_minutes,
            ai_summary_tone,
            activity_capture_enabled,
        })
    }

    pub fn save_app_preferences(
        &self,
        privacy_notice_accepted: bool,
        default_pomodoro_minutes: i64,
        ai_summary_tone: &str,
        activity_capture_enabled: bool,
    ) -> rusqlite::Result<()> {
        self.set_preference_value(
            "privacy_notice_accepted",
            if privacy_notice_accepted {
                "true"
            } else {
                "false"
            },
        )?;
        self.set_preference_value(
            "default_pomodoro_minutes",
            &default_pomodoro_minutes.clamp(1, 180).to_string(),
        )?;
        self.set_preference_value("ai_summary_tone", ai_summary_tone)?;
        self.set_preference_value(
            "activity_capture_enabled",
            if activity_capture_enabled {
                "true"
            } else {
                "false"
            },
        )?;
        Ok(())
    }

    fn preference_value(&self, key: &str) -> rusqlite::Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT value FROM app_preferences WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
    }

    fn set_preference_value(&self, key: &str, value: &str) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO app_preferences (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn pomodoro_event_count(&self, event_type: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM pomodoro_events WHERE event_type = ?1",
            params![event_type],
            |row| row.get(0),
        )
    }
}

fn activity_label(recorded_at: &str) -> String {
    DateTime::parse_from_rfc3339(recorded_at)
        .map(|value| value.format("%H:%M:%S").to_string())
        .unwrap_or_else(|_| recorded_at.to_string())
}

fn mask_api_key(api_key: &str) -> String {
    if api_key.trim().is_empty() {
        return String::new();
    }
    let tail = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("******{}", tail)
}

fn non_empty_or_none(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.into())
    }
}

fn normalize_saved_provider(provider: Option<&str>) -> &'static str {
    match provider {
        Some("custom") => "custom",
        _ => "deepseek",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_all_tables() {
        let database = Database::memory().expect("database should initialize");
        for table_name in [
            "sessions",
            "window_samples",
            "app_usage",
            "activity_events",
            "pomodoro_events",
            "daily_reports",
            "ai_settings",
            "chat_messages",
            "app_preferences",
        ] {
            assert!(database
                .table_exists(table_name)
                .expect("table lookup should work"));
        }
    }

    #[test]
    fn creates_and_ends_session() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");
        assert_eq!(session.status, "studying");
        assert!(session.ended_at.is_none());

        let ended = database
            .stop_session(session.id)
            .expect("session should end");
        assert_eq!(ended.status, "ended");
        assert!(ended.ended_at.is_some());
    }

    #[test]
    fn masks_ai_key_without_returning_secret() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("custom".into()),
                base_url: "https://api.example.test/v1".into(),
                api_key: "sk-test-secret-1234".into(),
                model: "demo-model".into(),
            })
            .expect("settings should save");

        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");
        let custom = masked_provider(&masked, "custom");
        assert_eq!(masked.active_provider, "custom");
        assert!(custom.configured);
        assert_eq!(custom.base_url, "https://api.example.test/v1");
        assert_eq!(custom.model, "demo-model");
        assert!(custom.api_key_masked.ends_with("1234"));
        assert!(!custom.api_key_masked.contains("secret"));
    }

    #[test]
    fn loads_full_ai_settings_only_for_backend_use() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("custom".into()),
                base_url: "https://api.example.test/v1".into(),
                api_key: "sk-test-secret-5678".into(),
                model: "demo-model".into(),
            })
            .expect("settings should save");

        let settings = database
            .get_ai_settings()
            .expect("settings query should work")
            .expect("settings should exist");

        assert_eq!(settings.base_url, "https://api.example.test/v1");
        assert_eq!(settings.api_key, "sk-test-secret-5678");
        assert_eq!(settings.model, "demo-model");
    }

    #[test]
    fn defaults_to_deepseek_provider_without_saved_key() {
        let database = Database::memory().expect("database should initialize");

        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");

        assert_eq!(masked.active_provider, "deepseek");
        assert_eq!(masked.providers.len(), 2);
        assert!(masked
            .providers
            .iter()
            .all(|item| item.provider != "builtin"));
        let deepseek = masked_provider(&masked, "deepseek");
        assert_eq!(deepseek.base_url, "https://api.deepseek.com");
        assert_eq!(deepseek.model, "deepseek-v4-pro");
        assert!(!deepseek.configured);
    }

    #[test]
    fn builtin_provider_is_migrated_to_deepseek() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("builtin".into()),
                base_url: "https://ignored.example/v1".into(),
                api_key: "".into(),
                model: "ignored-model".into(),
            })
            .expect("settings should save");

        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");

        assert_eq!(masked.active_provider, "deepseek");
        assert!(masked
            .providers
            .iter()
            .all(|item| item.provider != "builtin"));
    }

    #[test]
    fn deepseek_provider_exposes_model_choices_without_plain_key() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("deepseek".into()),
                base_url: "https://api.deepseek.com".into(),
                api_key: "sk-deepseek-secret-9999".into(),
                model: "deepseek-v4-flash".into(),
            })
            .expect("settings should save");

        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");

        let deepseek = masked_provider(&masked, "deepseek");
        assert_eq!(masked.active_provider, "deepseek");
        assert_eq!(deepseek.base_url, "https://api.deepseek.com");
        assert_eq!(deepseek.model, "deepseek-v4-flash");
        assert_eq!(
            deepseek.available_models,
            vec![
                "deepseek-v4-pro".to_string(),
                "deepseek-v4-flash".to_string()
            ]
        );
        assert!(!deepseek.base_url_editable);
        assert!(deepseek.api_key_required);
        assert!(!deepseek.api_key_masked.contains("secret"));
    }

    #[test]
    fn saves_deepseek_and_custom_keys_independently() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("deepseek".into()),
                base_url: "https://api.deepseek.com".into(),
                api_key: "sk-deepseek-only-1111".into(),
                model: "deepseek-v4-pro".into(),
            })
            .expect("deepseek settings should save");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("custom".into()),
                base_url: "https://api.custom.test/v1".into(),
                api_key: "sk-custom-only-2222".into(),
                model: "custom-model".into(),
            })
            .expect("custom settings should save");

        let deepseek = database
            .get_ai_settings_for_provider("deepseek")
            .expect("deepseek query should work")
            .expect("deepseek settings should exist");
        let custom = database
            .get_ai_settings_for_provider("custom")
            .expect("custom query should work")
            .expect("custom settings should exist");
        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");

        assert_eq!(masked.active_provider, "custom");
        assert_eq!(deepseek.api_key, "sk-deepseek-only-1111");
        assert_eq!(custom.api_key, "sk-custom-only-2222");
        assert!(masked_provider(&masked, "deepseek")
            .api_key_masked
            .ends_with("1111"));
        assert!(masked_provider(&masked, "custom")
            .api_key_masked
            .ends_with("2222"));
    }

    #[test]
    fn masked_custom_settings_stay_blank_when_not_configured() {
        let database = Database::memory().expect("database should initialize");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("custom".into()),
                base_url: "".into(),
                api_key: "sk-custom-key-3333".into(),
                model: "".into(),
            })
            .expect("settings should save");

        let masked = database
            .get_ai_settings_masked()
            .expect("masked settings should load");
        let custom = masked_provider(&masked, "custom");

        assert_eq!(custom.base_url, "");
        assert_eq!(custom.model, "");
    }

    fn masked_provider<'a>(
        masked: &'a AiSettingsMasked,
        provider: &str,
    ) -> &'a AiProviderSettingsMasked {
        masked
            .providers
            .iter()
            .find(|item| item.provider == provider)
            .expect("provider should be present")
    }

    #[test]
    fn stores_pomodoro_completion_event() {
        let database = Database::memory().expect("database should initialize");
        database
            .add_pomodoro_event("completed")
            .expect("event should save");

        assert_eq!(
            database
                .pomodoro_event_count("completed")
                .expect("event count should load"),
            1
        );
    }

    #[test]
    fn aggregates_window_samples_into_app_usage() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");

        for app_name in ["Code", "Code", "Browser"] {
            database
                .add_window_sample(
                    session.id,
                    &WindowSample {
                        app_name: app_name.into(),
                        window_title: "Test window".into(),
                        exe_path: None,
                        sampled_at: Utc::now().to_rfc3339(),
                    },
                )
                .expect("sample should save");
        }

        database
            .aggregate_app_usage(session.id)
            .expect("usage should aggregate");
        let usage = database
            .app_usage_for_session(session.id)
            .expect("usage should load");

        assert_eq!(usage[0].app_name, "Code");
        assert_eq!(usage[0].seconds, 2);
        assert_eq!(usage[1].app_name, "Browser");
        assert_eq!(usage[1].seconds, 1);
    }

    #[test]
    fn creates_daily_report_for_ended_session() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");
        let ended = database
            .stop_session(session.id)
            .expect("session should end");

        let report_id = database
            .create_daily_report(&ended, 12, 82, "[]", "[]", 0, None)
            .expect("report should save");

        assert!(report_id > 0);
        assert_eq!(database.report_count().expect("count should load"), 1);
    }

    #[test]
    fn stores_daily_report_total_seconds() {
        let database = Database::memory().expect("database should initialize");
        let session = database
            .add_session_with_times(
                "2026-05-22T08:00:00+00:00",
                Some("2026-05-22T08:01:30+00:00"),
                "ended",
            )
            .expect("session should save");

        let report_id = database
            .create_daily_report(&session, 90, 83, "[]", "[]", 0, None)
            .expect("report should save");

        assert_eq!(
            database
                .report_total_seconds(report_id)
                .expect("total should load"),
            90
        );
    }

    #[test]
    fn sums_today_study_seconds_for_multiple_sessions() {
        let database = Database::memory().expect("database should initialize");
        database
            .add_session_with_times(
                "2026-05-22T08:00:00+00:00",
                Some("2026-05-22T08:01:00+00:00"),
                "ended",
            )
            .expect("first session should save");
        database
            .add_session_with_times(
                "2026-05-22T09:00:00+00:00",
                Some("2026-05-22T09:02:00+00:00"),
                "ended",
            )
            .expect("second session should save");
        database
            .add_session_with_times(
                "2026-05-21T09:00:00+00:00",
                Some("2026-05-21T09:10:00+00:00"),
                "ended",
            )
            .expect("previous day session should save");

        let now = DateTime::parse_from_rfc3339("2026-05-22T12:00:00+00:00")
            .expect("date should parse")
            .with_timezone(&Utc);

        assert_eq!(
            database
                .today_study_seconds(now)
                .expect("today seconds should load"),
            180
        );
    }

    #[test]
    fn creates_daily_report_without_app_usage_samples() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");
        let ended = database
            .stop_session(session.id)
            .expect("session should end");
        let usage = database
            .app_usage_for_session(session.id)
            .expect("usage should load");

        let report_id = database
            .create_daily_report(
                &ended,
                0,
                80,
                &serde_json::to_string(&usage).expect("usage should serialize"),
                "[]",
                0,
                None,
            )
            .expect("report should save");

        assert!(usage.is_empty());
        assert_eq!(
            database
                .report_app_usage_json(report_id)
                .expect("usage json should load"),
            "[]"
        );
    }

    #[test]
    fn stores_chat_messages_for_report() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");
        let ended = database
            .stop_session(session.id)
            .expect("session should end");
        let report_id = database
            .create_daily_report(&ended, 0, 80, "[]", "[]", 0, None)
            .expect("report should save");

        database
            .add_chat_message(report_id, "user", "今天怎么样？")
            .expect("user message should save");
        database
            .add_chat_message(report_id, "assistant", "可以继续优化。")
            .expect("assistant message should save");

        let messages = database
            .chat_messages_for_report(report_id)
            .expect("messages should load");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[1].role, "assistant");
    }

    #[test]
    fn stores_and_aggregates_activity_events() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");

        database
            .add_activity_event(session.id, "keyboard", 3)
            .expect("keyboard events should save");
        database
            .add_activity_event(session.id, "mouse", 5)
            .expect("mouse events should save");
        database
            .add_activity_event(session.id, "keyboard", 2)
            .expect("more keyboard events should save");

        assert_eq!(
            database
                .activity_totals_for_session(session.id)
                .expect("activity totals should load"),
            (5, 5)
        );
    }

    #[test]
    fn rejects_unknown_activity_event_type() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");

        assert!(database.add_activity_event(session.id, "key_a", 1).is_err());
    }

    #[test]
    fn activity_points_can_be_saved_in_daily_report() {
        let database = Database::memory().expect("database should initialize");
        let session = database.start_session().expect("session should start");
        database
            .add_activity_event(session.id, "keyboard", 3)
            .expect("keyboard events should save");
        database
            .add_activity_event(session.id, "mouse", 1)
            .expect("mouse events should save");
        let ended = database
            .stop_session(session.id)
            .expect("session should end");
        let activity = database
            .activity_points_for_session(session.id)
            .expect("activity points should load");
        let report_id = database
            .create_daily_report(
                &ended,
                0,
                80,
                "[]",
                &serde_json::to_string(&activity).expect("activity should serialize"),
                0,
                None,
            )
            .expect("report should save");

        assert!(!activity.is_empty());
        assert!(database
            .get_report_context(report_id)
            .expect("report should load")
            .activity_json
            .contains("keyboard"));
    }

    #[test]
    fn stores_app_preferences() {
        let database = Database::memory().expect("database should initialize");

        let defaults = database
            .get_app_preferences()
            .expect("preferences should load");
        assert!(!defaults.privacy_notice_accepted);
        assert_eq!(defaults.default_pomodoro_minutes, 25);
        assert_eq!(defaults.ai_summary_tone, "witty");
        assert!(defaults.activity_capture_enabled);

        database
            .save_app_preferences(true, 40, "gentle", false)
            .expect("preferences should save");
        let saved = database
            .get_app_preferences()
            .expect("preferences should reload");
        assert!(saved.privacy_notice_accepted);
        assert_eq!(saved.default_pomodoro_minutes, 40);
        assert_eq!(saved.ai_summary_tone, "gentle");
        assert!(!saved.activity_capture_enabled);
    }

    #[test]
    fn lists_recent_daily_reports() {
        let database = Database::memory().expect("database should initialize");
        let first = database
            .add_session_with_times(
                "2026-05-22T08:00:00+00:00",
                Some("2026-05-22T08:10:00+00:00"),
                "ended",
            )
            .expect("first session should save");
        let second = database
            .add_session_with_times(
                "2026-05-22T09:00:00+00:00",
                Some("2026-05-22T09:20:00+00:00"),
                "ended",
            )
            .expect("second session should save");

        database
            .create_daily_report(&first, 600, 80, "[]", "[]", 0, Some("first"))
            .expect("first report should save");
        let second_report_id = database
            .create_daily_report(&second, 1200, 86, "[]", "[]", 1, Some("second"))
            .expect("second report should save");

        let reports = database
            .recent_daily_reports(1)
            .expect("reports should load");
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].id, second_report_id);
        assert_eq!(reports[0].ai_summary.as_deref(), Some("second"));
    }

    #[test]
    fn deleting_daily_report_keeps_session_time_and_removes_chat() {
        let database = Database::memory().expect("database should initialize");
        let session = database
            .add_session_with_times(
                "2026-05-22T08:00:00+00:00",
                Some("2026-05-22T08:10:00+00:00"),
                "ended",
            )
            .expect("session should save");
        let report_id = database
            .create_daily_report(&session, 600, 80, "[]", "[]", 0, Some("summary"))
            .expect("report should save");
        database
            .add_chat_message(report_id, "user", "hello")
            .expect("chat should save");

        database
            .delete_daily_report(report_id)
            .expect("report should delete");

        let now = DateTime::parse_from_rfc3339("2026-05-22T12:00:00+00:00")
            .expect("date should parse")
            .with_timezone(&Utc);
        assert_eq!(database.report_count().expect("count should load"), 0);
        assert_eq!(
            database
                .chat_message_count_for_report(report_id)
                .expect("chat count should load"),
            0
        );
        assert_eq!(
            database
                .today_study_seconds(now)
                .expect("today seconds should load"),
            600
        );
    }

    #[test]
    fn clearing_local_data_keeps_ai_settings_and_preferences() {
        let database = Database::memory().expect("database should initialize");
        let session = database
            .add_session_with_times(
                "2026-05-22T08:00:00+00:00",
                Some("2026-05-22T08:10:00+00:00"),
                "ended",
            )
            .expect("session should save");
        let report_id = database
            .create_daily_report(&session, 600, 80, "[]", "[]", 0, Some("summary"))
            .expect("report should save");
        database
            .add_chat_message(report_id, "user", "hello")
            .expect("chat should save");
        database
            .add_activity_event(session.id, "keyboard", 3)
            .expect("activity should save");
        database
            .add_pomodoro_event("completed")
            .expect("pomodoro event should save");
        database
            .save_ai_settings(&AiSettingsInput {
                provider: Some("custom".into()),
                base_url: "https://api.example.test/v1".into(),
                api_key: "sk-test-secret-5678".into(),
                model: "demo-model".into(),
            })
            .expect("settings should save");
        database
            .save_app_preferences(true, 40, "gentle", false)
            .expect("preferences should save");

        database.clear_local_data().expect("data should clear");

        let now = DateTime::parse_from_rfc3339("2026-05-22T12:00:00+00:00")
            .expect("date should parse")
            .with_timezone(&Utc);
        let settings = database
            .get_ai_settings_for_provider("custom")
            .expect("settings query should work")
            .expect("custom settings should remain");
        let preferences = database
            .get_app_preferences()
            .expect("preferences should remain");

        assert_eq!(database.report_count().expect("count should load"), 0);
        assert_eq!(
            database
                .chat_message_count_for_report(report_id)
                .expect("chat count should load"),
            0
        );
        assert_eq!(
            database
                .today_study_seconds(now)
                .expect("today seconds should load"),
            0
        );
        assert_eq!(settings.api_key, "sk-test-secret-5678");
        assert!(preferences.privacy_notice_accepted);
        assert_eq!(preferences.default_pomodoro_minutes, 40);
        assert!(!preferences.activity_capture_enabled);
    }

    #[test]
    fn closes_stale_studying_sessions_without_inflating_time() {
        let database = Database::memory().expect("database should initialize");
        let stale = database
            .add_session_with_times("2026-05-22T08:00:00+00:00", None, "studying")
            .expect("stale session should save");

        let changed = database
            .close_stale_studying_sessions()
            .expect("stale sessions should close");
        let closed = database
            .get_session(stale.id)
            .expect("closed session should load");
        let now = DateTime::parse_from_rfc3339("2026-05-22T12:00:00+00:00")
            .expect("date should parse")
            .with_timezone(&Utc);

        assert_eq!(changed, 1);
        assert_eq!(closed.status, "ended");
        assert_eq!(closed.ended_at.as_deref(), Some(closed.started_at.as_str()));
        assert_eq!(
            database
                .today_study_seconds(now)
                .expect("today seconds should load"),
            0
        );
        assert!(database
            .latest_studying_session_id()
            .expect("lookup should work")
            .is_none());
    }
}
