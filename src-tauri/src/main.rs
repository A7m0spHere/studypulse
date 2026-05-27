mod activity;
mod ai;
mod collector;
mod db;
mod pomodoro;

use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    thread::JoinHandle,
    time::Duration,
};

use activity::{start_activity_capture, ActivityHandle};
use ai::{AiMessage, AiSettings};
use chrono::{DateTime, NaiveTime, Utc};
use collector::sample_foreground_window;
use db::Database;
use pomodoro::{PomodoroMachine, PomodoroState, TickResult};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

type AppResult<T> = Result<T, String>;

struct AppState {
    db: Arc<Mutex<Database>>,
    data_dir: PathBuf,
    active_session_id: Mutex<Option<i64>>,
    pomodoro: Arc<Mutex<PomodoroMachine>>,
    sampler: Mutex<Option<SamplerHandle>>,
    activity: Mutex<Option<ActivityHandle>>,
}

struct SamplerHandle {
    stop: Arc<AtomicBool>,
    handle: JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUsage {
    pub app_name: String,
    pub exe_path: Option<String>,
    pub seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityPoint {
    pub label: String,
    pub keyboard: i64,
    pub mouse: i64,
}

#[derive(Debug, Clone, Serialize)]
struct DashboardState {
    session_status: String,
    today_study_seconds: i64,
    current_session_seconds: i64,
    current_app: String,
    current_window_title: String,
    keyboard_count: i64,
    mouse_count: i64,
    focus_score: i64,
    app_usage: Vec<AppUsage>,
    activity: Vec<ActivityPoint>,
    pomodoro: PomodoroState,
    active_report_id: Option<i64>,
    ai_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DailyReport {
    id: i64,
    session_id: i64,
    started_at: String,
    ended_at: String,
    total_seconds: i64,
    focus_score: i64,
    app_usage: Vec<AppUsage>,
    activity: Vec<ActivityPoint>,
    pomodoro_completed: i64,
    ai_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct AppPreferences {
    privacy_notice_accepted: bool,
    default_pomodoro_minutes: i64,
    ai_summary_tone: String,
    activity_capture_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AppPreferencesInput {
    privacy_notice_accepted: bool,
    default_pomodoro_minutes: i64,
    ai_summary_tone: String,
    activity_capture_enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AiSettingsInput {
    #[serde(default)]
    pub provider: Option<String>,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiSettingsMasked {
    pub active_provider: String,
    pub providers: Vec<AiProviderSettingsMasked>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AiProviderSettingsMasked {
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key_masked: String,
    pub configured: bool,
    pub available_models: Vec<String>,
    pub base_url_editable: bool,
    pub api_key_required: bool,
}

#[derive(Debug, Clone, Serialize)]
struct AiTestResult {
    ok: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct AiModelList {
    ok: bool,
    models: Vec<String>,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    id: i64,
    report_id: i64,
    role: String,
    content: String,
    created_at: String,
}

#[derive(Debug, Clone)]
pub struct ReportContext {
    id: i64,
    session_id: i64,
    started_at: String,
    ended_at: String,
    total_seconds: i64,
    focus_score: i64,
    app_usage_json: String,
    activity_json: String,
    pomodoro_completed: i64,
    ai_summary: Option<String>,
}

#[tauri::command]
fn start_session(state: State<AppState>) -> AppResult<Session> {
    if let Some(session_id) = *active_session(&state)? {
        return db(&state)?.get_session(session_id).map_err(to_string);
    }

    db(&state)?
        .close_stale_studying_sessions()
        .map_err(to_string)?;

    let session = db(&state)?.start_session().map_err(to_string)?;
    *active_session(&state)? = Some(session.id);
    start_sampler_if_needed(&state, session.id)?;
    start_activity_if_needed(&state, session.id)?;
    Ok(session)
}

#[tauri::command]
fn stop_session(state: State<AppState>) -> AppResult<DailyReport> {
    let session_id = {
        let mut active = active_session(&state)?;
        let session_id = if let Some(session_id) = *active {
            session_id
        } else {
            return Err("no active study session".to_string());
        };
        *active = None;
        session_id
    };

    stop_sampler(&state);
    stop_activity(&state);
    let session = db(&state)?.stop_session(session_id).map_err(to_string)?;
    db(&state)?
        .aggregate_app_usage(session_id)
        .map_err(to_string)?;
    let app_usage = db(&state)?
        .app_usage_for_session(session_id)
        .map_err(to_string)?;
    let pomodoro_completed = pomodoro_snapshot(&state).completed_count;
    let total_seconds = session_total_seconds(&session);
    let focus_score = focus_score(total_seconds, app_usage.len(), pomodoro_completed);
    let activity = db(&state)?
        .activity_points_for_session(session_id)
        .map_err(to_string)?;
    let app_usage_json = serde_json::to_string(&app_usage).map_err(to_string)?;
    let activity_json = serde_json::to_string(&activity).map_err(to_string)?;
    let report_id = db(&state)?
        .create_daily_report(
            &session,
            total_seconds,
            focus_score,
            &app_usage_json,
            &activity_json,
            pomodoro_completed,
            None,
        )
        .map_err(to_string)?;

    Ok(report_for_session(
        report_id,
        session,
        total_seconds,
        focus_score,
        app_usage,
        activity,
        pomodoro_completed,
        None,
    ))
}

#[tauri::command]
fn get_current_status(state: State<AppState>) -> DashboardState {
    dashboard_state(&state).unwrap_or_else(|error| {
        eprintln!("[StudyPulse dashboard] failed to load dashboard: {error}");
        empty_dashboard(pomodoro_snapshot(&state))
    })
}

#[tauri::command]
fn get_today_dashboard(state: State<AppState>) -> DashboardState {
    get_current_status(state)
}

#[tauri::command]
fn start_pomodoro(minutes: i64, state: State<AppState>) -> AppResult<PomodoroState> {
    let (snapshot, token) = {
        let mut machine = pomodoro(&state)?;
        machine.start(minutes)
    };

    spawn_pomodoro_timer(Arc::clone(&state.pomodoro), Arc::clone(&state.db), token);
    Ok(snapshot)
}

#[tauri::command]
fn pause_pomodoro(state: State<AppState>) -> AppResult<PomodoroState> {
    Ok(pomodoro(&state)?.pause())
}

#[tauri::command]
fn reset_pomodoro(state: State<AppState>) -> AppResult<PomodoroState> {
    Ok(pomodoro(&state)?.reset())
}

#[tauri::command]
fn save_ai_settings(settings: AiSettingsInput, state: State<AppState>) -> AppResult<()> {
    let settings = hydrate_saved_ai_key_if_needed(settings, &state)?;
    let canonical = canonical_ai_settings_input_clean(&settings)?;
    db(&state)?.save_ai_settings(&canonical).map_err(to_string)
}

#[tauri::command]
fn get_ai_settings_masked(state: State<AppState>) -> AppResult<AiSettingsMasked> {
    db(&state)?.get_ai_settings_masked().map_err(to_string)
}

#[tauri::command]
async fn test_ai_connection(
    settings: AiSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<AiTestResult> {
    let settings = hydrate_saved_ai_key_if_needed(settings, &state)?;
    let resolved = resolve_ai_settings(&settings)?;
    match ai::test_connection(&resolved).await {
        Ok(result) => Ok(AiTestResult {
            ok: true,
            message: match (result.model_count, result.chat_ok) {
                (Some(count), true) => {
                    format!(
                        "API 可用，检测到 {count} 个模型，当前模型 {} 可正常响应。",
                        resolved.model
                    )
                }
                (None, true) => {
                    format!(
                        "API 可用，当前模型 {} 可正常响应；该服务未返回模型列表。",
                        resolved.model
                    )
                }
                _ => "API 连接异常，请稍后重试。".into(),
            },
        }),
        Err(error) => Ok(AiTestResult {
            ok: false,
            message: error,
        }),
    }
}

#[tauri::command]
async fn list_ai_models(
    settings: AiSettingsInput,
    state: State<'_, AppState>,
) -> AppResult<AiModelList> {
    let settings = hydrate_saved_ai_key_if_needed(settings, &state)?;
    let resolved = resolve_ai_settings_for_models(&settings)?;
    match ai::list_models(&resolved).await {
        Ok(mut models) => {
            models.sort();
            models.dedup();
            Ok(AiModelList {
                ok: true,
                message: format!("检测到 {} 个可用模型。", models.len()),
                models,
            })
        }
        Err(error) => Ok(AiModelList {
            ok: false,
            models: Vec::new(),
            message: error,
        }),
    }
}

#[tauri::command(rename_all = "snake_case")]
async fn generate_ai_summary(
    report_id: i64,
    tone: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<String> {
    let (settings, report) = {
        let database = db(&state)?;
        (
            database.get_ai_settings().map_err(to_string)?,
            database.get_report_context(report_id).map_err(to_string)?,
        )
    };

    let Some(settings) = settings else {
        let summary = mock_ai_summary(&report);
        db(&state)?
            .update_report_summary(report_id, &summary)
            .map_err(to_string)?;
        return Ok(summary);
    };
    if settings.api_key.trim().is_empty() {
        let summary = mock_ai_summary(&report);
        db(&state)?
            .update_report_summary(report_id, &summary)
            .map_err(to_string)?;
        return Ok(summary);
    }

    let summary =
        ai::chat_completion(&settings, summary_messages_clean(&report, tone.as_deref())).await?;
    db(&state)?
        .update_report_summary(report_id, &summary)
        .map_err(to_string)?;
    Ok(summary)
}

#[tauri::command]
fn get_recent_reports(limit: Option<i64>, state: State<AppState>) -> AppResult<Vec<DailyReport>> {
    db(&state)?
        .recent_daily_reports(limit.unwrap_or(30))
        .map_err(to_string)
}

#[tauri::command(rename_all = "snake_case")]
fn delete_daily_report(report_id: i64, state: State<AppState>) -> AppResult<()> {
    db(&state)?
        .delete_daily_report(report_id)
        .map_err(to_string)
}

#[tauri::command]
fn get_data_dir(state: State<AppState>) -> String {
    state.data_dir.to_string_lossy().to_string()
}

#[tauri::command]
fn open_data_dir(state: State<AppState>) -> AppResult<()> {
    fs::create_dir_all(&state.data_dir).map_err(to_string)?;
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&state.data_dir)
            .spawn()
            .map_err(|error| format!("打开数据目录失败: {error}"))?;
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("当前版本仅支持在 Windows 上打开数据目录。".into())
    }
}

#[tauri::command]
fn clear_local_data(state: State<AppState>) -> AppResult<()> {
    if current_session_id(&state)?.is_some() {
        return Err("请先结束当前学习会话，再清空本地学习数据。".into());
    }
    stop_sampler(&state);
    stop_activity(&state);
    db(&state)?.clear_local_data().map_err(to_string)
}

#[tauri::command(rename_all = "snake_case")]
fn export_daily_report(
    report_id: i64,
    format: String,
    state: State<AppState>,
) -> AppResult<String> {
    let report = db(&state)?
        .get_report_context(report_id)
        .map_err(to_string)?;
    let extension = match format.as_str() {
        "txt" => "txt",
        "markdown" | "md" => "md",
        _ => return Err("导出格式只支持 txt 或 markdown。".into()),
    };
    let export_dir = state.data_dir.join("exports");
    fs::create_dir_all(&export_dir).map_err(to_string)?;
    let file_path = export_dir.join(format!("StudyPulse_Report_{}.{}", report.id, extension));
    let content = render_report_export(&report, extension == "md")?;
    fs::write(&file_path, content).map_err(to_string)?;
    Ok(file_path.to_string_lossy().to_string())
}

#[tauri::command]
fn get_app_preferences(state: State<AppState>) -> AppResult<AppPreferences> {
    db(&state)?.get_app_preferences().map_err(to_string)
}

#[tauri::command]
fn save_app_preferences(
    preferences: AppPreferencesInput,
    state: State<AppState>,
) -> AppResult<AppPreferences> {
    let minutes = preferences.default_pomodoro_minutes.clamp(1, 180);
    let tone = normalize_tone(&preferences.ai_summary_tone).to_string();
    db(&state)?
        .save_app_preferences(
            preferences.privacy_notice_accepted,
            minutes,
            &tone,
            preferences.activity_capture_enabled,
        )
        .map_err(to_string)?;
    db(&state)?.get_app_preferences().map_err(to_string)
}

#[tauri::command(rename_all = "snake_case")]
async fn chat_with_ai(
    report_id: i64,
    message: String,
    state: State<'_, AppState>,
) -> AppResult<ChatMessage> {
    let message = message.trim().to_string();
    if message.is_empty() {
        return Err("message cannot be empty".into());
    }

    let (settings, report, history) = {
        let database = db(&state)?;
        let settings = database.get_ai_settings().map_err(to_string)?;
        let report = database.get_report_context(report_id).map_err(to_string)?;
        let user_message = database
            .add_chat_message(report_id, "user", &message)
            .map_err(to_string)?;
        let mut history = database
            .chat_messages_for_report(report_id)
            .map_err(to_string)?;
        if !history.iter().any(|item| item.id == user_message.id) {
            history.push(user_message);
        }
        (settings, report, history)
    };

    let reply = if let Some(settings) = settings {
        if settings.api_key.trim().is_empty() {
            format!("Mock reply received: {message}")
        } else {
            ai::chat_completion(&settings, chat_messages_clean(&report, &history)).await?
        }
    } else {
        format!("Mock reply received: {message}")
    };

    db(&state)?
        .add_chat_message(report_id, "assistant", &reply)
        .map_err(to_string)
}

fn dashboard_state(state: &State<AppState>) -> AppResult<DashboardState> {
    let now = Utc::now();
    let active_id = current_session_id(state)?;
    let latest_sample = db(state)?.latest_window_sample().map_err(to_string)?;
    let today_study_seconds = db(state)?.today_study_seconds(now).map_err(to_string)?;
    let current_session_seconds = if let Some(session_id) = active_id {
        db(state)?
            .get_session(session_id)
            .map(|session| session_elapsed_seconds(&session, now))
            .map_err(to_string)?
    } else {
        0
    };
    let app_usage = if let Some(session_id) = active_id {
        db(state)?
            .app_usage_from_samples_for_session(session_id)
            .map_err(to_string)?
    } else {
        db(state)?
            .app_usage_from_samples_since(&today_start_utc(now))
            .map_err(to_string)?
    };
    let (keyboard_count, mouse_count, activity) = if let Some(session_id) = active_id {
        let (keyboard, mouse) = db(state)?
            .activity_totals_for_session(session_id)
            .map_err(to_string)?;
        let (pending_keyboard, pending_mouse) = pending_activity_counts(state);
        (
            keyboard + pending_keyboard,
            mouse + pending_mouse,
            db(state)?
                .activity_points_for_session(session_id)
                .map_err(to_string)?,
        )
    } else {
        (0, 0, Vec::new())
    };
    let focus_score = focus_score(
        today_study_seconds,
        app_usage.len(),
        pomodoro_snapshot(state).completed_count,
    );
    let active_report_id = db(state)?.latest_report_id().map_err(to_string)?;

    let current_app = latest_sample
        .as_ref()
        .map(|sample| sample.app_name.clone())
        .unwrap_or_else(|| "Not started".into());
    let current_window_title = latest_sample
        .map(|sample| sample.window_title)
        .unwrap_or_else(|| "Start a study session to record the active window".into());

    Ok(DashboardState {
        session_status: if active_id.is_some() {
            "studying".into()
        } else {
            "idle".into()
        },
        today_study_seconds,
        current_session_seconds,
        current_app,
        current_window_title,
        keyboard_count,
        mouse_count,
        focus_score,
        app_usage,
        activity,
        pomodoro: pomodoro_snapshot(state),
        active_report_id,
        ai_summary: None,
    })
}

fn empty_dashboard(pomodoro: PomodoroState) -> DashboardState {
    DashboardState {
        session_status: "idle".into(),
        today_study_seconds: 0,
        current_session_seconds: 0,
        current_app: "Not started".into(),
        current_window_title: "Start a study session to record the active window".into(),
        keyboard_count: 0,
        mouse_count: 0,
        focus_score: 0,
        app_usage: Vec::new(),
        activity: Vec::new(),
        pomodoro,
        active_report_id: None,
        ai_summary: None,
    }
}

fn report_for_session(
    id: i64,
    session: Session,
    total_seconds: i64,
    focus_score: i64,
    app_usage: Vec<AppUsage>,
    activity: Vec<ActivityPoint>,
    pomodoro_completed: i64,
    ai_summary: Option<String>,
) -> DailyReport {
    DailyReport {
        id,
        session_id: session.id,
        started_at: session.started_at,
        ended_at: session.ended_at.unwrap_or_else(now),
        total_seconds,
        focus_score,
        app_usage,
        activity,
        pomodoro_completed,
        ai_summary,
    }
}

fn render_report_export(report: &ReportContext, markdown: bool) -> AppResult<String> {
    let app_usage: Vec<AppUsage> = serde_json::from_str(&report.app_usage_json).unwrap_or_default();
    let activity: Vec<ActivityPoint> =
        serde_json::from_str(&report.activity_json).unwrap_or_default();
    let mut lines = Vec::new();

    if markdown {
        lines.push(format!("# StudyPulse 日报 #{}", report.id));
        lines.push(String::new());
        lines.push(format!("- 会话 ID：{}", report.session_id));
        lines.push(format!("- 开始时间：{}", report.started_at));
        lines.push(format!("- 结束时间：{}", report.ended_at));
        lines.push(format!(
            "- 学习时长：{}",
            human_seconds(report.total_seconds)
        ));
        lines.push(format!("- 专注度：{}", report.focus_score));
        lines.push(format!("- 番茄钟完成数：{}", report.pomodoro_completed));
        lines.push(String::new());
        lines.push("## 应用排行".into());
        if app_usage.is_empty() {
            lines.push("- 暂无应用采样数据。".into());
        } else {
            for item in app_usage.iter().take(10) {
                lines.push(format!(
                    "- {}：{}",
                    item.app_name,
                    human_seconds(item.seconds)
                ));
            }
        }
        lines.push(String::new());
        lines.push("## 活跃度".into());
        if activity.is_empty() {
            lines.push("- 暂无键鼠活跃度趋势。".into());
        } else {
            for item in activity.iter().take(20) {
                lines.push(format!(
                    "- {}：键盘 {}，鼠标 {}",
                    item.label, item.keyboard, item.mouse
                ));
            }
        }
        lines.push(String::new());
        lines.push("## AI 总结".into());
        lines.push(
            report
                .ai_summary
                .clone()
                .unwrap_or_else(|| "尚未生成 AI 总结。".into()),
        );
    } else {
        lines.push(format!("StudyPulse 日报 #{}", report.id));
        lines.push(format!("会话 ID：{}", report.session_id));
        lines.push(format!("开始时间：{}", report.started_at));
        lines.push(format!("结束时间：{}", report.ended_at));
        lines.push(format!("学习时长：{}", human_seconds(report.total_seconds)));
        lines.push(format!("专注度：{}", report.focus_score));
        lines.push(format!("番茄钟完成数：{}", report.pomodoro_completed));
        lines.push(String::new());
        lines.push("应用排行：".into());
        if app_usage.is_empty() {
            lines.push("暂无应用采样数据。".into());
        } else {
            for item in app_usage.iter().take(10) {
                lines.push(format!(
                    "{} - {}",
                    item.app_name,
                    human_seconds(item.seconds)
                ));
            }
        }
        lines.push(String::new());
        lines.push("活跃度：".into());
        if activity.is_empty() {
            lines.push("暂无键鼠活跃度趋势。".into());
        } else {
            for item in activity.iter().take(20) {
                lines.push(format!(
                    "{} - 键盘 {}，鼠标 {}",
                    item.label, item.keyboard, item.mouse
                ));
            }
        }
        lines.push(String::new());
        lines.push("AI 总结：".into());
        lines.push(
            report
                .ai_summary
                .clone()
                .unwrap_or_else(|| "尚未生成 AI 总结。".into()),
        );
    }

    Ok(lines.join("\n"))
}

fn db<'a>(state: &'a State<'_, AppState>) -> AppResult<MutexGuard<'a, Database>> {
    state.db.lock().map_err(|_| "database lock failed".into())
}

fn active_session<'a>(state: &'a State<'_, AppState>) -> AppResult<MutexGuard<'a, Option<i64>>> {
    state
        .active_session_id
        .lock()
        .map_err(|_| "session lock failed".into())
}

fn pomodoro<'a>(state: &'a State<'_, AppState>) -> AppResult<MutexGuard<'a, PomodoroMachine>> {
    state
        .pomodoro
        .lock()
        .map_err(|_| "pomodoro lock failed".into())
}

fn pomodoro_snapshot(state: &State<AppState>) -> PomodoroState {
    state
        .pomodoro
        .lock()
        .map(|machine| machine.snapshot())
        .unwrap_or_default()
}

fn current_session_id(state: &State<AppState>) -> AppResult<Option<i64>> {
    Ok(*active_session(state)?)
}

fn session_total_seconds(session: &Session) -> i64 {
    let Ok(started_at) = DateTime::parse_from_rfc3339(&session.started_at) else {
        return 0;
    };
    let ended_at = session
        .ended_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .unwrap_or(started_at);
    (ended_at - started_at).num_seconds().max(0)
}

fn session_elapsed_seconds(session: &Session, now: DateTime<Utc>) -> i64 {
    let Ok(started_at) = DateTime::parse_from_rfc3339(&session.started_at) else {
        return 0;
    };
    (now - started_at.with_timezone(&Utc)).num_seconds().max(0)
}

fn focus_score(total_seconds: i64, app_count: usize, pomodoro_completed: i64) -> i64 {
    let duration_bonus = (total_seconds / 900).min(8);
    let switch_penalty = (app_count.saturating_sub(3) as i64 * 4).min(24);
    let pomodoro_bonus = (pomodoro_completed * 4).min(12);
    (80 + duration_bonus + pomodoro_bonus - switch_penalty).clamp(0, 100)
}

fn today_start_utc(now: DateTime<Utc>) -> String {
    now.date_naive()
        .and_time(NaiveTime::MIN)
        .and_utc()
        .to_rfc3339()
}

fn mock_ai_summary(report: &ReportContext) -> String {
    format!(
        "Mock summary for report #{}: studied for {}, focus score {}, pomodoros {}.",
        report.id,
        human_seconds(report.total_seconds),
        report.focus_score,
        report.pomodoro_completed
    )
}

fn canonical_ai_settings_input_clean(settings: &AiSettingsInput) -> AppResult<AiSettingsInput> {
    let provider = normalize_ai_provider(settings.provider.as_deref());
    match provider {
        "deepseek" => {
            let model = settings.model.trim();
            if !deepseek_models().contains(&model) {
                return Err("DeepSeek 模型只能选择 deepseek-v4-pro 或 deepseek-v4-flash。".into());
            }
            if settings.api_key.trim().is_empty() {
                return Err("请填写 DeepSeek API Key。".into());
            }
            Ok(AiSettingsInput {
                provider: Some("deepseek".into()),
                base_url: "https://api.deepseek.com".into(),
                api_key: settings.api_key.trim().into(),
                model: model.into(),
            })
        }
        "custom" => {
            if settings.base_url.trim().is_empty() {
                return Err("请填写自定义 API Base URL。".into());
            }
            if settings.model.trim().is_empty() {
                return Err("请填写自定义模型名称。".into());
            }
            if settings.api_key.trim().is_empty() {
                return Err("请填写自定义 API Key。".into());
            }
            Ok(AiSettingsInput {
                provider: Some("custom".into()),
                base_url: settings.base_url.trim().into(),
                api_key: settings.api_key.trim().into(),
                model: settings.model.trim().into(),
            })
        }
        _ => Err("未知的 API 供应商。".into()),
    }
}

#[allow(dead_code)]
fn canonical_ai_settings_input(settings: &AiSettingsInput) -> AppResult<AiSettingsInput> {
    canonical_ai_settings_input_clean(settings)
}

fn hydrate_saved_ai_key_if_needed(
    settings: AiSettingsInput,
    state: &State<'_, AppState>,
) -> AppResult<AiSettingsInput> {
    let provider = normalize_ai_provider(settings.provider.as_deref());
    if !settings.api_key.trim().is_empty() {
        return Ok(settings);
    }

    let masked = db(state)?.get_ai_settings_masked().map_err(to_string)?;
    let provider_state = masked
        .providers
        .iter()
        .find(|item| item.provider == provider);
    if !provider_state.map(|item| item.configured).unwrap_or(false) {
        return Ok(settings);
    }

    let Some(saved) = db(state)?
        .get_ai_settings_for_provider(provider)
        .map_err(to_string)?
    else {
        return Ok(settings);
    };

    Ok(AiSettingsInput {
        api_key: saved.api_key,
        ..settings
    })
}

fn resolve_ai_settings(settings: &AiSettingsInput) -> AppResult<AiSettings> {
    let canonical = canonical_ai_settings_input_clean(settings)?;
    Ok(AiSettings {
        base_url: canonical.base_url,
        api_key: canonical.api_key,
        model: canonical.model,
    })
}

fn resolve_ai_settings_for_models(settings: &AiSettingsInput) -> AppResult<AiSettings> {
    let provider = normalize_ai_provider(settings.provider.as_deref());
    let (base_url, api_key) = match provider {
        "deepseek" => (
            "https://api.deepseek.com".to_string(),
            settings.api_key.trim().to_string(),
        ),
        "custom" => {
            if settings.base_url.trim().is_empty() {
                return Err("请填写自定义 API Base URL。".into());
            }
            (
                settings.base_url.trim().to_string(),
                settings.api_key.trim().to_string(),
            )
        }
        _ => return Err("未知的 API 供应商。".into()),
    };
    if api_key.is_empty() {
        return Err("请先填写 API Key，或保存后使用已保存的 Key 检测。".into());
    }
    Ok(AiSettings {
        base_url,
        api_key,
        model: settings.model.trim().to_string(),
    })
}

fn normalize_ai_provider(provider: Option<&str>) -> &'static str {
    match provider.unwrap_or("deepseek") {
        "deepseek" => "deepseek",
        "custom" => "custom",
        _ => "unknown",
    }
}

fn deepseek_models() -> [&'static str; 2] {
    ["deepseek-v4-pro", "deepseek-v4-flash"]
}

fn summary_messages_clean(report: &ReportContext, tone: Option<&str>) -> Vec<AiMessage> {
    vec![
        AiMessage {
            role: "system".into(),
            content: format!(
                "你是 StudyPulse 的学习总结助手。请用中文输出，不要编造未提供的数据。总结语气：{}。",
                tone_instruction_clean(tone)
            ),
        },
        AiMessage {
            role: "user".into(),
            content: report_prompt_clean(report, tone),
        },
    ]
}

fn chat_messages_clean(report: &ReportContext, history: &[ChatMessage]) -> Vec<AiMessage> {
    let mut messages = vec![
        AiMessage {
            role: "system".into(),
            content:
                "你是 StudyPulse 的学习复盘聊天助手。请基于日报上下文回答，保持简短、具体、友好。"
                    .into(),
        },
        AiMessage {
            role: "user".into(),
            content: report_prompt_clean(report, None),
        },
        AiMessage {
            role: "assistant".into(),
            content: report
                .ai_summary
                .clone()
                .unwrap_or_else(|| "我已经看到这份学习日报，可以继续聊。".into()),
        },
    ];

    messages.extend(history.iter().map(|message| AiMessage {
        role: message.role.clone(),
        content: message.content.clone(),
    }));
    messages
}

fn report_prompt_clean(report: &ReportContext, tone: Option<&str>) -> String {
    format!(
        "请根据以下本地学习日报生成总结：\n\
         report_id: {}\n\
         session_id: {}\n\
         started_at: {}\n\
         ended_at: {}\n\
         total_seconds: {}\n\
         focus_score: {}\n\
         pomodoro_completed: {}\n\
         app_usage_json: {}\n\
         activity_json: {}\n\
         总结语气: {}\n\
         要求：先用一句话概括，再给 2-3 条具体观察，最后给一句符合语气的建议或鼓励。",
        report.id,
        report.session_id,
        report.started_at,
        report.ended_at,
        report.total_seconds,
        report.focus_score,
        report.pomodoro_completed,
        report.app_usage_json,
        report.activity_json,
        tone_label_clean(tone)
    )
}

fn tone_label_clean(tone: Option<&str>) -> &'static str {
    match normalize_tone(tone.unwrap_or("witty")) {
        "gentle" => "温和鼓励",
        "normal" => "正常复盘",
        "strict" => "严格监督",
        _ => "轻微吐槽",
    }
}

fn tone_instruction_clean(tone: Option<&str>) -> &'static str {
    match normalize_tone(tone.unwrap_or("witty")) {
        "gentle" => "温和鼓励，重点肯定今天做得好的地方，少批评",
        "normal" => "正常复盘，客观指出表现、问题和下一步建议",
        "strict" => "严格监督，直说拖延和分心问题，但不要羞辱用户",
        _ => "轻微吐槽，语气可以有一点幽默，但要友好和有帮助",
    }
}

#[allow(dead_code)]
fn summary_messages(report: &ReportContext, tone: Option<&str>) -> Vec<AiMessage> {
    vec![
        AiMessage {
            role: "system".into(),
            content: format!(
                "你是 StudyPulse 的学习总结助手。请用中文输出，不要编造未提供的数据。总结语气：{}。",
                tone_instruction(tone)
            ),
        },
        AiMessage {
            role: "user".into(),
            content: report_prompt(report, tone),
        },
    ]
}

#[allow(dead_code)]
fn chat_messages(report: &ReportContext, history: &[ChatMessage]) -> Vec<AiMessage> {
    let mut messages = vec![
        AiMessage {
            role: "system".into(),
            content:
                "你是 StudyPulse 的学习复盘聊天助手。请基于日报上下文回答，保持简短、具体、友好。"
                    .into(),
        },
        AiMessage {
            role: "user".into(),
            content: report_prompt(report, None),
        },
        AiMessage {
            role: "assistant".into(),
            content: report
                .ai_summary
                .clone()
                .unwrap_or_else(|| "我已经看到这份学习日报，可以继续聊。".into()),
        },
    ];

    messages.extend(history.iter().map(|message| AiMessage {
        role: message.role.clone(),
        content: message.content.clone(),
    }));
    messages
}

#[allow(dead_code)]
fn report_prompt(report: &ReportContext, tone: Option<&str>) -> String {
    format!(
        "请根据以下本地学习日报生成总结：\n\
         report_id: {}\n\
         session_id: {}\n\
         started_at: {}\n\
         ended_at: {}\n\
         total_seconds: {}\n\
         focus_score: {}\n\
         pomodoro_completed: {}\n\
         app_usage_json: {}\n\
         activity_json: {}\n\
         总结语气: {}\n\
         要求：先一句话概括，再给 2-3 条具体观察，最后给一句符合语气的建议或鼓励。",
        report.id,
        report.session_id,
        report.started_at,
        report.ended_at,
        report.total_seconds,
        report.focus_score,
        report.pomodoro_completed,
        report.app_usage_json,
        report.activity_json,
        tone_label(tone)
    )
}

fn normalize_tone(tone: &str) -> &str {
    match tone {
        "gentle" | "normal" | "witty" | "strict" => tone,
        _ => "witty",
    }
}

#[allow(dead_code)]
fn tone_label(tone: Option<&str>) -> &'static str {
    match normalize_tone(tone.unwrap_or("witty")) {
        "gentle" => "温和鼓励",
        "normal" => "正常复盘",
        "strict" => "严格监督",
        _ => "轻微吐槽",
    }
}

#[allow(dead_code)]
fn tone_instruction(tone: Option<&str>) -> &'static str {
    match normalize_tone(tone.unwrap_or("witty")) {
        "gentle" => "温和鼓励，重点肯定今天做得好的地方，少批评",
        "normal" => "正常复盘，客观指出表现、问题和下一步建议",
        "strict" => "严格监督，直说拖延和分心问题，但不要羞辱用户",
        _ => "轻微吐槽，语气可以有一点幽默，但要友好和有帮助",
    }
}

fn human_seconds(seconds: i64) -> String {
    let minutes = seconds / 60;
    let remaining_seconds = seconds % 60;
    format!("{minutes}m {remaining_seconds}s")
}

fn start_sampler_if_needed(state: &State<AppState>, session_id: i64) -> AppResult<()> {
    let mut sampler = state
        .sampler
        .lock()
        .map_err(|_| "sampler lock failed".to_string())?;

    if sampler.is_some() {
        println!("[StudyPulse collector] sampler already running");
        return Ok(());
    }

    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let thread_db = Arc::clone(&state.db);

    println!("[StudyPulse collector] starting sampler for session {session_id}");
    let handle = thread::spawn(move || {
        while !thread_stop.load(Ordering::SeqCst) {
            match sample_foreground_window() {
                Ok(sample) => {
                    println!(
                        "[StudyPulse collector] sample app='{}' title='{}'",
                        sample.app_name, sample.window_title
                    );
                    if let Ok(database) = thread_db.lock() {
                        if let Err(error) = database.add_window_sample(session_id, &sample) {
                            eprintln!("[StudyPulse collector] failed to save sample: {error}");
                        }
                    } else {
                        eprintln!("[StudyPulse collector] database lock failed");
                    }
                }
                Err(error) => {
                    eprintln!("[StudyPulse collector] sample failed: {error}");
                }
            }

            thread::sleep(Duration::from_secs(1));
        }

        println!("[StudyPulse collector] sampler stopped for session {session_id}");
    });

    *sampler = Some(SamplerHandle { stop, handle });
    Ok(())
}

fn stop_sampler(state: &State<AppState>) {
    let sampler = state.sampler.lock().ok().and_then(|mut value| value.take());
    if let Some(sampler) = sampler {
        println!("[StudyPulse collector] stopping sampler");
        sampler.stop.store(true, Ordering::SeqCst);
        if sampler.handle.join().is_err() {
            eprintln!("[StudyPulse collector] sampler thread panicked while stopping");
        }
    }
}

fn start_activity_if_needed(state: &State<AppState>, session_id: i64) -> AppResult<()> {
    if !db(state)?
        .get_app_preferences()
        .map_err(to_string)?
        .activity_capture_enabled
    {
        println!("[StudyPulse activity] activity capture disabled by preferences");
        return Ok(());
    }

    let mut activity = state
        .activity
        .lock()
        .map_err(|_| "activity lock failed".to_string())?;

    if activity.is_some() {
        println!("[StudyPulse activity] activity capture already running");
        return Ok(());
    }

    match start_activity_capture(session_id, Arc::clone(&state.db)) {
        Ok(handle) => {
            println!("[StudyPulse activity] starting activity capture for session {session_id}");
            *activity = Some(handle);
        }
        Err(error) => {
            eprintln!("[StudyPulse activity] activity capture unavailable: {error}");
        }
    }

    Ok(())
}

fn stop_activity(state: &State<AppState>) {
    let activity = state
        .activity
        .lock()
        .ok()
        .and_then(|mut value| value.take());
    if let Some(activity) = activity {
        println!("[StudyPulse activity] stopping activity capture");
        activity.stop();
    }
}

fn pending_activity_counts(state: &State<AppState>) -> (i64, i64) {
    state
        .activity
        .lock()
        .ok()
        .and_then(|activity| activity.as_ref().map(|handle| handle.pending_counts()))
        .unwrap_or((0, 0))
}

fn spawn_pomodoro_timer(
    pomodoro: Arc<Mutex<PomodoroMachine>>,
    db: Arc<Mutex<Database>>,
    token: u64,
) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));

        let tick = match pomodoro.lock() {
            Ok(mut machine) => machine.tick_one_second(token),
            Err(_) => break,
        };

        match tick {
            TickResult::Completed(_) => {
                if let Ok(db) = db.lock() {
                    let _ = db.add_pomodoro_event("completed");
                }
                break;
            }
            TickResult::Cancelled => break,
            TickResult::Running | TickResult::Waiting => {}
        }
    });
}

fn app_data_dir(handle: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let dir = handle.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app_data_dir(app.handle())?;
            let db_path = data_dir.join("studypulse.sqlite3");
            let database = Database::open(&db_path)?;
            database.close_stale_studying_sessions()?;
            app.manage(AppState {
                db: Arc::new(Mutex::new(database)),
                data_dir,
                active_session_id: Mutex::new(None),
                pomodoro: Arc::new(Mutex::new(PomodoroMachine::new())),
                sampler: Mutex::new(None),
                activity: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_session,
            stop_session,
            get_current_status,
            get_today_dashboard,
            start_pomodoro,
            pause_pomodoro,
            reset_pomodoro,
            save_ai_settings,
            get_ai_settings_masked,
            test_ai_connection,
            list_ai_models,
            generate_ai_summary,
            chat_with_ai,
            get_recent_reports,
            delete_daily_report,
            get_data_dir,
            open_data_dir,
            clear_local_data,
            export_daily_report,
            get_app_preferences,
            save_app_preferences
        ])
        .run(tauri::generate_context!())
        .expect("error while running StudyPulse");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_session_total_seconds_from_timestamps() {
        let session = Session {
            id: 1,
            started_at: "2026-05-22T08:00:00+00:00".into(),
            ended_at: Some("2026-05-22T08:01:30+00:00".into()),
            status: "ended".into(),
        };

        assert_eq!(session_total_seconds(&session), 90);
    }

    #[test]
    fn calculates_current_session_elapsed_from_system_time() {
        let session = Session {
            id: 1,
            started_at: "2026-05-22T08:00:00+00:00".into(),
            ended_at: None,
            status: "studying".into(),
        };
        let now = DateTime::parse_from_rfc3339("2026-05-22T08:00:05+00:00")
            .expect("date should parse")
            .with_timezone(&Utc);

        assert_eq!(session_elapsed_seconds(&session, now), 5);
    }

    #[test]
    fn focus_score_stays_in_range() {
        assert_eq!(focus_score(0, 100, 0), 56);
        assert_eq!(focus_score(10 * 3600, 0, 10), 100);
    }

    #[test]
    fn report_prompt_contains_local_report_context() {
        let report = ReportContext {
            id: 7,
            session_id: 3,
            started_at: "2026-05-22T08:00:00+00:00".into(),
            ended_at: "2026-05-22T08:30:00+00:00".into(),
            total_seconds: 1800,
            focus_score: 86,
            app_usage_json: r#"[{"app_name":"Code","seconds":1200}]"#.into(),
            activity_json: "[]".into(),
            pomodoro_completed: 1,
            ai_summary: None,
        };

        let prompt = report_prompt(&report, Some("witty"));
        assert!(prompt.contains("report_id: 7"));
        assert!(prompt.contains("Code"));
        assert!(prompt.contains("focus_score: 86"));
        assert!(prompt.contains("轻微吐槽"));
    }

    #[test]
    fn renders_daily_report_markdown_export() {
        let report = ReportContext {
            id: 7,
            session_id: 3,
            started_at: "2026-05-22T08:00:00+00:00".into(),
            ended_at: "2026-05-22T08:30:00+00:00".into(),
            total_seconds: 1800,
            focus_score: 86,
            app_usage_json: r#"[{"app_name":"Code","seconds":1200}]"#.into(),
            activity_json: r#"[{"label":"08:05","keyboard":12,"mouse":3}]"#.into(),
            pomodoro_completed: 1,
            ai_summary: Some("状态不错，继续保持。".into()),
        };

        let content = render_report_export(&report, true).expect("report should render");

        assert!(content.contains("# StudyPulse 日报 #7"));
        assert!(content.contains("Code"));
        assert!(content.contains("状态不错"));
    }

    #[test]
    fn tone_values_are_normalized() {
        assert_eq!(tone_label(Some("gentle")), "温和鼓励");
        assert_eq!(tone_label(Some("normal")), "正常复盘");
        assert_eq!(tone_label(Some("strict")), "严格监督");
        assert_eq!(tone_label(Some("unknown")), "轻微吐槽");
    }
}
