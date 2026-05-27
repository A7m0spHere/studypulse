import { CheckCircle2, FolderOpen, Loader2, Search, Trash2, X, XCircle } from "lucide-react";
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
  onDataCleared?: () => void;
}

type AiProvider = AiSettingsInput["provider"];
type AiForms = Record<AiProvider, AiSettingsInput>;

const DEEPSEEK_MODELS = ["deepseek-v4-pro", "deepseek-v4-flash"];

const DEFAULT_FORMS: AiForms = {
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
  onDataCleared,
}: SettingsModalProps) {
  const [activeProvider, setActiveProvider] = useState<AiProvider>("deepseek");
  const [forms, setForms] = useState<AiForms>(cloneDefaultForms());
  const [masked, setMasked] = useState<AiSettingsMasked | null>(null);
  const [message, setMessage] = useState("");
  const [testMessage, setTestMessage] = useState("");
  const [testing, setTesting] = useState(false);
  const [detectingModels, setDetectingModels] = useState(false);
  const [customModels, setCustomModels] = useState<string[]>([]);
  const [dataDir, setDataDir] = useState("");
  const [clearingData, setClearingData] = useState(false);

  useEffect(() => {
    if (!open) return;
    setMessage("");
    setTestMessage("");
    setCustomModels([]);
    api.getDataDir().then(setDataDir).catch(() => setDataDir(""));
    api
      .getAiSettingsMasked()
      .then((value) => {
        setMasked(value);
        setActiveProvider(value.active_provider);
        setForms(formsFromMasked(value));
        const custom = value.providers.find((item) => item.provider === "custom");
        setCustomModels(custom?.available_models ?? []);
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
      setMessage("已保存。DeepSeek 和自定义 API 的 Key 会分别保存在本机配置中，界面不会显示明文。");
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

  async function detectCustomModels() {
    setDetectingModels(true);
    setTestMessage("");
    try {
      const result = await api.listAiModels(forms.custom);
      setTestMessage(result.message);
      if (result.ok) {
        setCustomModels(result.models);
        if (!forms.custom.model.trim() && result.models.length > 0) {
          setForms((current) => ({
            ...current,
            custom: { ...current.custom, model: result.models[0] },
          }));
        }
      }
    } catch (error) {
      setTestMessage(String(error));
    } finally {
      setDetectingModels(false);
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

  async function openDataDir() {
    setMessage("");
    try {
      await api.openDataDir();
    } catch (error) {
      setMessage(String(error));
    }
  }

  async function clearLocalData() {
    if (!window.confirm("确定清空本地学习数据吗？AI 设置和隐私确认状态会保留。")) return;
    if (!window.confirm("再次确认：该操作会删除学习会话、窗口采样、应用排行、活跃度、番茄钟事件、日报和聊天记录。")) return;
    setClearingData(true);
    setMessage("");
    try {
      await api.clearLocalData();
      setMessage("本地学习数据已清空，AI 设置和隐私确认状态已保留。");
      onDataCleared?.();
    } catch (error) {
      setMessage(String(error));
    } finally {
      setClearingData(false);
    }
  }

  const deepseek = activeProvider === "deepseek";
  const custom = activeProvider === "custom";
  const apiKeyPlaceholder = apiKeyPlaceholderFor(activeMasked);

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
                  关闭后不启动键盘 hook 和鼠标轮询，可用于排查鼠标卡顿；学习会话、窗口采集和日报仍可使用。
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
            <div className="grid gap-2 sm:grid-cols-2">
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
                自定义 OpenAI 兼容 API
              </button>
            </div>
            <p className="mt-2 text-xs leading-5 text-ink/55">
              DeepSeek 固定 Base URL，模型从预设中选择，Key 由用户填写。自定义模式不预设任何地址或模型，可检测 /models 后再选择。
            </p>
          </div>

          <label className="field">
            <span>API URL (Base URL)</span>
            <input
              value={activeSettings.base_url}
              disabled={!custom}
              onChange={(event) => updateActiveForm({ base_url: event.target.value })}
              placeholder={custom ? "例如 https://api.example.com/v1" : activeSettings.base_url}
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
              <div className="space-y-2">
                {customModels.length > 0 ? (
                  <select
                    value={customModels.includes(activeSettings.model) ? activeSettings.model : ""}
                    onChange={(event) => updateActiveForm({ model: event.target.value })}
                  >
                    <option value="">手动输入模型名</option>
                    {customModels.map((model) => (
                      <option key={model} value={model}>
                        {model}
                      </option>
                    ))}
                  </select>
                ) : null}
                <input
                  value={activeSettings.model}
                  onChange={(event) => updateActiveForm({ model: event.target.value })}
                  placeholder="可先检测模型，也可以手动输入"
                />
              </div>
            )}
          </label>

          <label className="field">
            <span>API Key</span>
            <input
              value={activeSettings.api_key}
              onChange={(event) => updateActiveForm({ api_key: event.target.value })}
              type="password"
              placeholder={apiKeyPlaceholder}
            />
          </label>

          <div className="rounded-md border border-line bg-white/70 p-3 text-sm leading-6 text-ink/70">
            <p>
              AI 总结只会在你主动生成总结或继续聊天时，将本地日报摘要发送到当前选择的 API。API Key
              不会以明文返回前端，也不会写入日志。
            </p>
            <button className="mt-2 text-sm font-semibold text-moss" onClick={onShowPrivacy}>
              查看完整隐私说明
            </button>
          </div>

          <div className="rounded-md border border-line bg-white/70 p-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-ink">本地数据</p>
              <p className="mt-1 break-all text-xs leading-5 text-ink/60">
                {dataDir || "正在读取数据目录..."}
              </p>
              <p className="mt-2 text-xs leading-5 text-ink/55">
                清空本地学习数据会删除会话、窗口采样、应用排行、活跃度、番茄钟事件、日报和聊天记录；AI 设置与隐私确认状态会保留。
              </p>
            </div>
            <div className="mt-3 flex flex-wrap gap-2">
              <button className="secondary-button" type="button" onClick={openDataDir}>
                <FolderOpen size={16} />
                打开数据目录
              </button>
              <button
                className="danger-button"
                type="button"
                onClick={clearLocalData}
                disabled={clearingData}
              >
                {clearingData ? <Loader2 className="animate-spin" size={16} /> : <Trash2 size={16} />}
                清空本地数据
              </button>
            </div>
          </div>

          {message ? <p className="text-sm text-moss">{message}</p> : null}
          {testMessage ? (
            <p className="flex items-center gap-2 text-sm text-ink/75">
              {testMessage.includes("可用") || testMessage.includes("检测到") || testMessage.includes("available") ? (
                <CheckCircle2 size={16} />
              ) : (
                <XCircle size={16} />
              )}
              {testMessage}
            </p>
          ) : null}

          <div className="flex flex-wrap justify-end gap-2">
            <button className="secondary-button" onClick={onClose}>
              先不设置
            </button>
            {custom ? (
              <button className="secondary-button" onClick={detectCustomModels} disabled={detectingModels}>
                {detectingModels ? <Loader2 className="animate-spin" size={16} /> : <Search size={16} />}
                检测可用模型
              </button>
            ) : null}
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
    deepseek: { ...DEFAULT_FORMS.deepseek },
    custom: { ...DEFAULT_FORMS.custom },
  };
}

function apiKeyPlaceholderFor(provider: AiProviderSettingsMasked | undefined) {
  if (provider?.configured) return provider.api_key_masked;
  return "sk-...";
}
