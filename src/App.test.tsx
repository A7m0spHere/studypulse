// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import type { AppPreferences, DashboardState } from "./lib/types";

class ResizeObserverMock {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver = ResizeObserverMock as unknown as typeof ResizeObserver;

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (command: string, args?: Record<string, unknown>) => invokeMock(command, args),
}));

function dashboard(overrides: Partial<DashboardState> = {}): DashboardState {
  return {
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
    active_report_id: null,
    ai_summary: null,
    ...overrides,
  };
}

const preferences: AppPreferences = {
  privacy_notice_accepted: true,
  default_pomodoro_minutes: 25,
  ai_summary_tone: "witty",
  activity_capture_enabled: true,
};

describe("App", () => {
  beforeEach(() => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    vi.clearAllMocks();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_current_status" || command === "get_today_dashboard") {
        return Promise.resolve(dashboard());
      }
      if (command === "get_app_preferences") {
        return Promise.resolve(preferences);
      }
      if (command === "get_recent_reports") {
        return Promise.resolve([]);
      }
      if (command === "get_data_dir") return Promise.resolve("C:\\Users\\tester\\AppData\\Roaming\\StudyPulse");
      if (command === "open_data_dir" || command === "clear_local_data") return Promise.resolve(undefined);
      if (command === "export_daily_report") return Promise.resolve("C:\\Users\\tester\\report.md");
      return Promise.resolve({});
    });
  });

  afterEach(() => {
    cleanup();
    delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  it("renders idle dashboard with empty app usage and activity", async () => {
    render(<App />);

    expect(await screen.findByText("StudyPulse")).toBeInTheDocument();
    expect(await screen.findByText("待开始")).toBeInTheDocument();
    expect(await screen.findByText("今日学习")).toBeInTheDocument();
    expect(await screen.findByText("生成 AI 总结")).toBeDisabled();
    expect(await screen.findByText("开始学习并切换几个窗口后，这里会显示应用使用时长排行。")).toBeInTheDocument();
  });

  it("renders studying state and disables start while enabling stop", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_current_status" || command === "get_today_dashboard") {
        return Promise.resolve(
          dashboard({
            session_status: "studying",
            today_study_seconds: 125,
            current_session_seconds: 5,
            current_app: "Code",
            current_window_title: "StudyPulse - main.rs",
            keyboard_count: 8,
            mouse_count: 3,
            focus_score: 82,
            app_usage: [{ app_name: "Code", exe_path: null, seconds: 125 }],
            activity: [{ label: "10:00:05", keyboard: 8, mouse: 3 }],
          }),
        );
      }
      if (command === "get_app_preferences") return Promise.resolve(preferences);
      if (command === "get_recent_reports") return Promise.resolve([]);
      return Promise.resolve({});
    });

    render(<App />);

    expect(await screen.findByText("学习中")).toBeInTheDocument();
    expect(await screen.findByText("11")).toBeInTheDocument();
    expect(await screen.findByText("00:05")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /开始学习/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: /结束学习/ })).not.toBeDisabled();
  });

  it("renders ended-capable AI state when a report id exists", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_current_status" || command === "get_today_dashboard") {
        return Promise.resolve(
          dashboard({
            active_report_id: 12,
            ai_summary: "今天状态不错，继续保持。",
          }),
        );
      }
      if (command === "get_app_preferences") return Promise.resolve(preferences);
      if (command === "get_recent_reports") return Promise.resolve([]);
      return Promise.resolve({});
    });

    render(<App />);

    expect(await screen.findByText("今天状态不错，继续保持。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "生成 AI 总结" })).not.toBeDisabled();
  });

  it("deletes a history report without changing dashboard totals", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_current_status" || command === "get_today_dashboard") {
        return Promise.resolve(
          dashboard({
            today_study_seconds: 600,
            current_session_seconds: 0,
          }),
        );
      }
      if (command === "get_app_preferences") return Promise.resolve(preferences);
      if (command === "get_recent_reports") {
        return Promise.resolve([
          {
            id: 42,
            session_id: 7,
            started_at: "2026-05-23T09:00:00+08:00",
            ended_at: "2026-05-23T09:10:00+08:00",
            total_seconds: 600,
            focus_score: 82,
            app_usage: [],
            activity: [],
            pomodoro_completed: 0,
            ai_summary: null,
          },
        ]);
      }
      if (command === "delete_daily_report") return Promise.resolve(undefined);
      if (command === "export_daily_report") return Promise.resolve("C:\\Users\\tester\\StudyPulse_Report_42.md");
      return Promise.resolve({});
    });

    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: /历史日报/ }));
    fireEvent.click(await screen.findByRole("button", { name: /删除/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("delete_daily_report", { report_id: 42 });
    });
    expect(screen.getAllByText("10:00").length).toBeGreaterThan(0);
  });

  it("exports a history report as markdown", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_current_status" || command === "get_today_dashboard") return Promise.resolve(dashboard());
      if (command === "get_app_preferences") return Promise.resolve(preferences);
      if (command === "get_recent_reports") {
        return Promise.resolve([
          {
            id: 42,
            session_id: 7,
            started_at: "2026-05-23T09:00:00+08:00",
            ended_at: "2026-05-23T09:10:00+08:00",
            total_seconds: 600,
            focus_score: 82,
            app_usage: [{ app_name: "Code", exe_path: null, seconds: 300 }],
            activity: [],
            pomodoro_completed: 1,
            ai_summary: "今天完成了一次稳定的学习。",
          },
        ]);
      }
      if (command === "export_daily_report") return Promise.resolve("C:\\Users\\tester\\StudyPulse_Report_42.md");
      return Promise.resolve({});
    });

    render(<App />);

    fireEvent.click(await screen.findByRole("button", { name: /鍘嗗彶鏃ユ姤|历史日报/ }));
    fireEvent.click(await screen.findByRole("button", { name: /导出 MD/ }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("export_daily_report", {
        report_id: 42,
        format: "markdown",
      });
    });
  });
});
