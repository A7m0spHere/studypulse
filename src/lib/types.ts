export type SessionStatus = "idle" | "studying" | "paused" | "ended";

export interface Session {
  id: number;
  started_at: string;
  ended_at?: string | null;
  status: SessionStatus;
}

export interface AppUsage {
  app_name: string;
  exe_path?: string | null;
  seconds: number;
}

export interface ActivityPoint {
  label: string;
  keyboard: number;
  mouse: number;
}

export interface PomodoroState {
  status: "idle" | "running" | "paused" | "completed";
  total_seconds: number;
  remaining_seconds: number;
  completed_count: number;
}

export interface DashboardState {
  session_status: SessionStatus;
  today_study_seconds: number;
  current_session_seconds: number;
  current_app: string;
  current_window_title: string;
  keyboard_count: number;
  mouse_count: number;
  focus_score: number;
  app_usage: AppUsage[];
  activity: ActivityPoint[];
  pomodoro: PomodoroState;
  active_report_id?: number | null;
  ai_summary?: string | null;
}

export interface DailyReport {
  id: number;
  session_id: number;
  started_at: string;
  ended_at: string;
  total_seconds: number;
  focus_score: number;
  app_usage: AppUsage[];
  activity: ActivityPoint[];
  pomodoro_completed: number;
  ai_summary?: string | null;
}

export type AiSummaryTone = "gentle" | "normal" | "witty" | "strict";

export interface AppPreferences {
  privacy_notice_accepted: boolean;
  default_pomodoro_minutes: number;
  ai_summary_tone: AiSummaryTone;
  activity_capture_enabled: boolean;
}

export interface AiSettingsInput {
  provider: "builtin" | "deepseek" | "custom";
  base_url: string;
  api_key: string;
  model: string;
}

export interface AiSettingsMasked {
  active_provider: "builtin" | "deepseek" | "custom";
  providers: AiProviderSettingsMasked[];
}

export interface AiProviderSettingsMasked {
  provider: "builtin" | "deepseek" | "custom";
  base_url: string;
  model: string;
  api_key_masked: string;
  configured: boolean;
  available_models: string[];
  base_url_editable: boolean;
  api_key_required: boolean;
}

export interface AiTestResult {
  ok: boolean;
  message: string;
}

export interface ChatMessage {
  id: number;
  report_id: number;
  role: "user" | "assistant" | "system";
  content: string;
  created_at: string;
}
