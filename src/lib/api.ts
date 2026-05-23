import { invoke } from "@tauri-apps/api/core";
import type {
  AiSettingsInput,
  AiSettingsMasked,
  AiTestResult,
  AppPreferences,
  AiSummaryTone,
  ChatMessage,
  DailyReport,
  DashboardState,
  PomodoroState,
  Session,
} from "./types";

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const emptyDashboard: DashboardState = {
  session_status: "idle",
  today_study_seconds: 0,
  current_session_seconds: 0,
  current_app: "Not started",
  current_window_title: "Start a study session to record the active window",
  keyboard_count: 0,
  mouse_count: 0,
  focus_score: 0,
  app_usage: [],
  activity: [],
  pomodoro: {
    status: "idle",
    total_seconds: 25 * 60,
    remaining_seconds: 25 * 60,
    completed_count: 0,
  },
};

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    if (command === "get_current_status" || command === "get_today_dashboard") {
      return emptyDashboard as T;
    }
    throw new Error("StudyPulse needs to run inside the Tauri desktop app for this action.");
  }

  return invoke<T>(command, args);
}

export const api = {
  startSession: () => call<Session>("start_session"),
  stopSession: () => call<DailyReport>("stop_session"),
  getCurrentStatus: () => call<DashboardState>("get_current_status"),
  getTodayDashboard: () => call<DashboardState>("get_today_dashboard"),
  startPomodoro: (minutes: number) => call<PomodoroState>("start_pomodoro", { minutes }),
  pausePomodoro: () => call<PomodoroState>("pause_pomodoro"),
  resetPomodoro: () => call<PomodoroState>("reset_pomodoro"),
  saveAiSettings: (settings: AiSettingsInput) => call<void>("save_ai_settings", { settings }),
  getAiSettingsMasked: () => call<AiSettingsMasked>("get_ai_settings_masked"),
  testAiConnection: (settings: AiSettingsInput) =>
    call<AiTestResult>("test_ai_connection", { settings }),
  generateAiSummary: (reportId: number, tone?: AiSummaryTone) =>
    call<string>("generate_ai_summary", { report_id: reportId, tone }),
  chatWithAi: (reportId: number, message: string) =>
    call<ChatMessage>("chat_with_ai", { report_id: reportId, message }),
  getRecentReports: (limit = 30) => call<DailyReport[]>("get_recent_reports", { limit }),
  deleteDailyReport: (reportId: number) => call<void>("delete_daily_report", { report_id: reportId }),
  getAppPreferences: () => call<AppPreferences>("get_app_preferences"),
  saveAppPreferences: (preferences: AppPreferences) =>
    call<AppPreferences>("save_app_preferences", { preferences }),
};

export function formatDuration(seconds: number) {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;

  if (h > 0) return `${h}h ${m.toString().padStart(2, "0")}m`;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}
