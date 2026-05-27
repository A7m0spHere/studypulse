// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppPreferences } from "../lib/types";
import { SettingsModal } from "./SettingsModal";

const apiMock = vi.hoisted(() => ({
  getAiSettingsMasked: vi.fn(),
  saveAiSettings: vi.fn(),
  testAiConnection: vi.fn(),
  listAiModels: vi.fn(),
  getDataDir: vi.fn(),
  openDataDir: vi.fn(),
  clearLocalData: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  api: apiMock,
}));

const preferences: AppPreferences = {
  privacy_notice_accepted: true,
  default_pomodoro_minutes: 25,
  ai_summary_tone: "witty",
  activity_capture_enabled: true,
};

function renderModal() {
  return render(
    <SettingsModal
      open
      onClose={vi.fn()}
      onShowPrivacy={vi.fn()}
      preferences={preferences}
      onSavePreferences={vi.fn()}
      onDataCleared={vi.fn()}
    />,
  );
}

describe("SettingsModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiMock.getAiSettingsMasked.mockResolvedValue({
      active_provider: "deepseek",
      providers: [
        {
          provider: "deepseek",
          base_url: "https://api.deepseek.com",
          model: "deepseek-v4-pro",
          api_key_masked: "******1111",
          configured: true,
          available_models: ["deepseek-v4-pro", "deepseek-v4-flash"],
          base_url_editable: false,
          api_key_required: true,
        },
        {
          provider: "custom",
          base_url: "https://api.example.com/v1",
          model: "custom-model",
          api_key_masked: "******2222",
          configured: true,
          available_models: [],
          base_url_editable: true,
          api_key_required: true,
        },
      ],
    });
    apiMock.saveAiSettings.mockResolvedValue(undefined);
    apiMock.testAiConnection.mockResolvedValue({
      ok: true,
      message: "API 可用，当前模型 deepseek-v4-pro 可正常响应。",
    });
    apiMock.listAiModels.mockResolvedValue({
      ok: true,
      models: ["demo-model-a", "demo-model-b"],
      message: "检测到 2 个可用模型。",
    });
    apiMock.getDataDir.mockResolvedValue("C:\\Users\\tester\\AppData\\Roaming\\StudyPulse");
    apiMock.openDataDir.mockResolvedValue(undefined);
    apiMock.clearLocalData.mockResolvedValue(undefined);
  });

  afterEach(() => {
    cleanup();
  });

  it("removes the builtin public API template", async () => {
    renderModal();

    expect(await screen.findByRole("button", { name: "DeepSeek" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "自定义 OpenAI 兼容 API" })).toBeInTheDocument();
    expect(screen.queryByText("内置公益 API")).not.toBeInTheDocument();
  });

  it("shows DeepSeek model choices and requires the user key field", async () => {
    renderModal();

    fireEvent.click(await screen.findByRole("button", { name: "DeepSeek" }));

    expect(screen.getByDisplayValue("https://api.deepseek.com")).toBeDisabled();
    expect(screen.getByRole("option", { name: "deepseek-v4-pro" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "deepseek-v4-flash" })).toBeInTheDocument();
    expect(screen.getByPlaceholderText("******1111")).not.toBeDisabled();
  });

  it("tests the current form values without saving them", async () => {
    renderModal();

    fireEvent.click(await screen.findByRole("button", { name: "测试 API" }));

    await waitFor(() => expect(apiMock.testAiConnection).toHaveBeenCalledTimes(1));
    expect(apiMock.saveAiSettings).not.toHaveBeenCalled();
    expect(await screen.findByText(/API 可用/)).toBeInTheDocument();
  });

  it("keeps DeepSeek and custom keys separated while switching templates", async () => {
    renderModal();

    fireEvent.click(await screen.findByRole("button", { name: "DeepSeek" }));
    expect(screen.getByPlaceholderText("******1111")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "自定义 OpenAI 兼容 API" }));
    expect(screen.getByPlaceholderText("******2222")).toBeInTheDocument();
    expect(screen.getByDisplayValue("https://api.example.com/v1")).not.toBeDisabled();
  });

  it("detects custom models and allows selecting one", async () => {
    renderModal();

    fireEvent.click(await screen.findByRole("button", { name: "自定义 OpenAI 兼容 API" }));
    fireEvent.click(screen.getByRole("button", { name: "检测可用模型" }));

    await waitFor(() => expect(apiMock.listAiModels).toHaveBeenCalledTimes(1));
    expect(await screen.findByRole("option", { name: "demo-model-a" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "demo-model-b" })).toBeInTheDocument();
  });

  it("keeps custom fields blank when no custom values are configured", async () => {
    apiMock.getAiSettingsMasked.mockResolvedValueOnce({
      active_provider: "custom",
      providers: [
        {
          provider: "deepseek",
          base_url: "https://api.deepseek.com",
          model: "deepseek-v4-pro",
          api_key_masked: "",
          configured: false,
          available_models: ["deepseek-v4-pro", "deepseek-v4-flash"],
          base_url_editable: false,
          api_key_required: true,
        },
        {
          provider: "custom",
          base_url: "",
          model: "",
          api_key_masked: "",
          configured: false,
          available_models: [],
          base_url_editable: true,
          api_key_required: true,
        },
      ],
    });

    renderModal();

    expect(await screen.findByPlaceholderText("例如 https://api.example.com/v1")).not.toBeDisabled();
    expect(screen.getByPlaceholderText("可先检测模型，也可以手动输入")).not.toBeDisabled();
  });
  it("shows local data tools and clears data after double confirmation", async () => {
    vi.spyOn(window, "confirm").mockReturnValue(true);
    renderModal();

    expect(await screen.findByText("本地数据")).toBeInTheDocument();
    expect(await screen.findByText("C:\\Users\\tester\\AppData\\Roaming\\StudyPulse")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /清空本地数据/ }));

    await waitFor(() => expect(apiMock.clearLocalData).toHaveBeenCalledTimes(1));
  });
});
