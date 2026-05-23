import { CheckCircle2, Loader2, XCircle, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import type {
  AiProviderSettingsMasked,
  AiSettingsInput,
  AiSettingsMasked,
  AppPreferences,
} from "../lib/types";

interface SettingsModalProps {
  open: boolean;
  onClose: () => void;
  onShowPrivacy: () => void;
  preferences: AppPreferences;
  onSavePreferences: (preferences: AppPreferences) => Promise<void>;
}

type AiProvider = AiSettingsInput["provider"];
type AiForms = Record<AiProvider, AiSettingsInput>;

const DEEPSEEK_MODELS = ["deepseek-v4-pro", "deepseek-v4-flash"];

const DEFAULT_FORMS: AiForms = {
  builtin: {
    provider: "builtin",
    base_url: "https://new.xinjianya.top/v1",
    api_key: "",
    model: "deepseek-ai/deepseek-v4-pro",
  },
  deepseek: {
    provider: "deepseek",
    base_url: "https://api.deepseek.com",
    api_key: "",
    model: DEEPSEEK_MODELS[0],
  },
  custom: {
    provider: "custom",
    base_url: "",
    api_key: "",
    model: "",
  },
};

export function SettingsModal({
  open,
  onClose,
  onShowPrivacy,
  preferences,
  onSavePreferences,
}: SettingsModalProps) {
  const [activeProvider, setActiveProvider] = useState<AiProvider>("builtin");
  const [forms, setForms] = useState<AiForms>(cloneDefaultForms());
  const [masked, setMasked] = useState<AiSettingsMasked | null>(null);
  const [message, setMessage] = useState("");
  const [testMessage, setTestMessage] = useState("");
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    if (!open) return;
    setMessage("");
    setTestMessage("");
    api
      .getAiSettingsMasked()
      .then((value) => {
        setMasked(value);
        setActiveProvider(value.active_provider);
        setForms(formsFromMasked(value));
      })
      .catch((error) => setMessage(String(error)));
  }, [open]);

  const activeSettings = forms[activeProvider];
  const activeMasked = useMemo(
    () => masked?.providers.find((item) => item.provider === activeProvider),
    [activeProvider, masked],
  );

  if (!open) return null;

  function updateActiveForm(patch: Partial<AiSettingsInput>) {
    setForms((current) => ({
      ...current,
      [activeProvider]: {
        ...current[activeProvider],
        ...patch,
      },
    }));
  }

  async function save() {
    setMessage("");
    setTestMessage("");
    try {
      await api.saveAiSettings(activeSettings);
      setMessage("已保存。每个模板的 API Key 会单独保存在本机配置中，界面不会显示明文。");
      setForms((current) => ({
        ...current,
        [activeProvider]: {
          ...current[activeProvider],
          api_key: "",
        },
      }));
      const nextMasked = await api.getAiSettingsMasked();
      setMasked(nextMasked);
      setActiveProvider(nextMasked.active_provider);
      setForms(formsFromMasked(nextMasked));
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function testConnection() {
    setTesting(true);
    setTestMessage("");
    try {
      const result = await api.testAiConnection(activeSettings);
      setTestMessage(result.message);
    } catch (error) {
      setTestMessage(String(error));
    } finally {
      setTesting(false);
    }
  }

  async function toggleActivityCapture() {
    await onSavePreferences({
      ...preferences,
      activity_capture_enabled: !preferences.activity_capture_enabled,
    });
    setMessage(
      preferences.activity_capture_enabled
        ? "已关闭键鼠活跃度统计。下次开始学习时生效。"
        : "已开启键鼠活跃度统计。下次开始学习时生效。",
    );
  }

  const builtin = activeProvider === "builtin";
  const deepseek = activeProvider === "deepseek";
  const custom = activeProvider === "custom";
  const apiKeyPlaceholder = apiKeyPlaceholderFor(activeMasked, builtin);

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-ink/30 p-6">
      <section className="w-full max-w-xl rounded-lg border border-line bg-paper shadow-panel">
        <header className="flex items-center justify-between border-b border-line px-5 py-4">
          <div>
            <h2 className="text-lg font-semibold text-ink">设置</h2>
            <p className="text-sm text-ink/60">AI 配置、采集开关和隐私边界</p>
          </div>
          <button className="icon-button" onClick={onClose} aria-label="关闭设置">
            <X size={18} />
          </button>
        </header>

        <div className="max-h-[78vh] space-y-4 overflow-y-auto p-5">
          <div className="rounded-md border border-line bg-white/70 p-3">
            <div className="flex items-center justify-between gap-4">
              <div>
                <p className="text-sm font-semibold text-ink">键鼠活跃度统计</p>
                <p className="mt-1 text-sm leading-6 text-ink/60">
                  关闭后不会启动键盘 hook 和鼠标轮询，可用于排查鼠标卡顿；学习会话、窗口采集和日报仍可用。
                </p>
              </div>
              <button
                className={preferences.activity_capture_enabled ? "primary-button" : "secondary-button"}
                onClick={toggleActivityCapture}
              >
                {preferences.activity_capture_enabled ? "已开启" : "已关闭"}
              </button>
            </div>
          </div>

          <div className="rounded-md border border-line bg-white/70 p-3">
            <p className="mb-2 text-sm font-semibold text-ink">AI 供应商</p>
            <div className="grid gap-2 sm:grid-cols-3">
              <button
                className={builtin ? "primary-button" : "secondary-button"}
                onClick={() => setActiveProvider("builtin")}
              >
                内置公益 API
              </button>
              <button
                className={deepseek ? "primary-button" : "secondary-button"}
                onClick={() => setActiveProvider("deepseek")}
              >
                DeepSeek
              </button>
              <button
                className={custom ? "primary-button" : "secondary-button"}
                onClick={() => setActiveProvider("custom")}
              >
                自定义
              </button>
            </div>
            <p className="mt-2 text-xs leading-5 text-ink/55">
              每个模板的 API Key 单独保存。内置公益 API 为只读预设，但连接不稳定，建议用户自己接入其他 API。
              DeepSeek 固定 Base URL；自定义模式需要自行填写 OpenAI 兼容配置。
            </p>
          </div>

          <label className="field">
            <span>API URL (Base URL)</span>
            <input
              value={activeSettings.base_url}
              disabled={!custom}
              onChange={(event) => updateActiveForm({ base_url: event.target.value })}
              placeholder={custom ? "请输入 OpenAI 兼容 Base URL" : activeSettings.base_url}
            />
          </label>

          <label className="field">
            <span>模型名称</span>
            {deepseek ? (
              <select
                value={activeSettings.model}
                onChange={(event) => updateActiveForm({ model: event.target.value })}
              >
                {DEEPSEEK_MODELS.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            ) : (
              <input
                value={activeSettings.model}
                disabled={builtin}
                onChange={(event) => updateActiveForm({ model: event.target.value })}
                placeholder={custom ? "请输入模型名称" : activeSettings.model}
              />
            )}
          </label>

          <label className="field">
            <span>API Key</span>
            <input
              value={activeSettings.api_key}
              disabled={builtin}
              onChange={(event) => updateActiveForm({ api_key: event.target.value })}
              type="password"
              placeholder={apiKeyPlaceholder}
            />
          </label>

          <div className="rounded-md border border-line bg-white/70 p-3 text-sm leading-6 text-ink/70">
            <p>
              AI 总结只会在你主动点击生成总结或继续聊天时，把本地日报摘要发送到当前选择的 API。
              API Key 不会以明文返回前端，也不会写入日志。
            </p>
            <button className="mt-2 text-sm font-semibold text-moss" onClick={onShowPrivacy}>
              查看完整隐私说明
            </button>
          </div>

          {message ? <p className="text-sm text-moss">{message}</p> : null}
          {testMessage ? (
            <p className="flex items-center gap-2 text-sm text-ink/75">
              {testMessage.includes("可用") ? <CheckCircle2 size={16} /> : <XCircle size={16} />}
              {testMessage}
            </p>
          ) : null}

          <div className="flex flex-wrap justify-end gap-2">
            <button className="secondary-button" onClick={onClose}>
              先不设置
            </button>
            <button className="secondary-button" onClick={testConnection} disabled={testing}>
              {testing ? <Loader2 className="animate-spin" size={16} /> : null}
              测试 API
            </button>
            <button className="primary-button" onClick={save}>
              保存
            </button>
          </div>
        </div>
      </section>
    </div>
  );
}

function formsFromMasked(masked: AiSettingsMasked): AiForms {
  return masked.providers.reduce<AiForms>((forms, provider) => {
    const defaults = DEFAULT_FORMS[provider.provider];
    forms[provider.provider] = {
      provider: provider.provider,
      base_url: provider.base_url.trim() || defaults.base_url,
      api_key: "",
      model: provider.model.trim() || defaults.model,
    };
    return forms;
  }, cloneDefaultForms());
}

function cloneDefaultForms(): AiForms {
  return {
    builtin: { ...DEFAULT_FORMS.builtin },
    deepseek: { ...DEFAULT_FORMS.deepseek },
    custom: { ...DEFAULT_FORMS.custom },
  };
}

function apiKeyPlaceholderFor(provider: AiProviderSettingsMasked | undefined, builtin: boolean) {
  if (builtin) return provider?.api_key_masked || "内置 Key";
  if (provider?.configured) return provider.api_key_masked;
  return "sk-...";
}
