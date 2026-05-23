// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SettingsModal } from "./SettingsModal";
import type { AppPreferences } from "../lib/types";

const apiMock = vi.hoisted(() => ({
  getAiSettingsMasked: vi.fn(),
  saveAiSettings: vi.fn(),
  testAiConnection: vi.fn(),
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
    />,
  );
}

describe("SettingsModal", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiMock.getAiSettingsMasked.mockResolvedValue({
      active_provider: "builtin",
      providers: [
        {
          provider: "builtin",
          base_url: "https://new.xinjianya.top/v1",
          model: "deepseek-ai/deepseek-v4-pro",
          api_key_masked: "******nWl6",
          configured: true,
          available_models: [],
          base_url_editable: false,
          api_key_required: false,
        },
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
      message: "API 可用，模型 deepseek-ai/deepseek-v4-pro 返回正常。",
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("keeps builtin preset read-only", async () => {
    renderModal();

    const baseUrl = await screen.findByDisplayValue("https://new.xinjianya.top/v1");
    const model = screen.getByDisplayValue("deepseek-ai/deepseek-v4-pro");
    const key = screen.getByPlaceholderText("******nWl6");

    expect(baseUrl).toBeDisabled();
    expect(model).toBeDisabled();
    expect(key).toBeDisabled();
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

    fireEvent.click(screen.getByRole("button", { name: "自定义" }));
    expect(screen.getByPlaceholderText("******2222")).toBeInTheDocument();
    expect(screen.getByDisplayValue("https://api.example.com/v1")).not.toBeDisabled();
  });

  it("keeps custom fields blank when no custom values are configured", async () => {
    apiMock.getAiSettingsMasked.mockResolvedValueOnce({
      active_provider: "custom",
      providers: [
        {
          provider: "builtin",
          base_url: "https://new.xinjianya.top/v1",
          model: "deepseek-ai/deepseek-v4-pro",
          api_key_masked: "******nWl6",
          configured: true,
          available_models: [],
          base_url_editable: false,
          api_key_required: false,
        },
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

    expect(await screen.findByPlaceholderText("请输入 OpenAI 兼容 Base URL")).not.toBeDisabled();
    expect(screen.getByPlaceholderText("请输入模型名称")).not.toBeDisabled();
  });
});
