import {
  Activity,
  Brain,
  Clock3,
  Coffee,
  History,
  MessageSquareText,
  Play,
  RefreshCw,
  Settings,
  ShieldCheck,
  Square,
  TimerReset,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  Bar,
  BarChart,
  CartesianGrid,
  Line,
  LineChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { SettingsModal } from "./components/SettingsModal";
import { StatCard } from "./components/StatCard";
import { api, formatDuration } from "./lib/api";
import type { AiSummaryTone, AppPreferences, ChatMessage, DailyReport, DashboardState } from "./lib/types";

const defaultPreferences: AppPreferences = {
  privacy_notice_accepted: false,
  default_pomodoro_minutes: 25,
  ai_summary_tone: "witty",
  activity_capture_enabled: true,
};

const toneLabels: Record<AiSummaryTone, string> = {
  gentle: "温和鼓励",
  normal: "正常复盘",
  witty: "轻微吐槽",
  strict: "严格监督",
};

function todayLabel() {
  return new Intl.DateTimeFormat("zh-CN", {
    month: "long",
    day: "numeric",
    weekday: "short",
  }).format(new Date());
}

function statusLabel(status?: string) {
  if (status === "studying") return "学习中";
  if (status === "ended") return "已结束";
  return "待开始";
}

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export default function App() {
  const [dashboard, setDashboard] = useState<DashboardState | null>(null);
  const [lastReport, setLastReport] = useState<DailyReport | null>(null);
  const [reports, setReports] = useState<DailyReport[]>([]);
  const [preferences, setPreferences] = useState<AppPreferences>(defaultPreferences);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [privacyOpen, setPrivacyOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [chatInput, setChatInput] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);

  const pomodoroMinutes = preferences.default_pomodoro_minutes;

  async function refresh() {
    try {
      setDashboard(await api.getCurrentStatus());
    } catch (refreshError) {
      setError(String(refreshError));
    }
  }

  async function loadPreferences() {
    try {
      const next = await api.getAppPreferences();
      setPreferences(next);
      if (!next.privacy_notice_accepted) setPrivacyOpen(true);
    } catch (preferenceError) {
      setError(String(preferenceError));
    }
  }

  async function loadReports() {
    try {
      setReports(await api.getRecentReports(30));
    } catch (reportError) {
      setError(String(reportError));
    }
  }

  useEffect(() => {
    refresh();
    loadPreferences();
    loadReports();
    const timer = window.setInterval(refresh, 1000);
    return () => window.clearInterval(timer);
  }, []);

  const isStudying = dashboard?.session_status === "studying";
  const reportId = lastReport?.id ?? dashboard?.active_report_id ?? null;
  const aiSummary = lastReport?.ai_summary ?? dashboard?.ai_summary;
  const topApps = dashboard?.app_usage.slice(0, 6) ?? [];
  const focusScore = dashboard?.focus_score ?? 0;
  const focusTone = focusScore >= 70 ? "稳定专注" : focusScore >= 40 ? "状态一般" : "刚刚起步";

  const activityData = useMemo(() => {
    if (dashboard?.activity.length) return dashboard.activity;
    return [{ label: "现在", keyboard: 0, mouse: 0 }];
  }, [dashboard]);

  async function runAction<T>(action: () => Promise<T>, after?: (value: T) => void) {
    setBusy(true);
    setError("");
    try {
      const value = await action();
      after?.(value);
      await refresh();
      await loadReports();
    } catch (actionError) {
      setError(String(actionError));
    } finally {
      setBusy(false);
    }
  }

  async function savePreferences(next: AppPreferences) {
    setPreferences(next);
    try {
      setPreferences(await api.saveAppPreferences(next));
    } catch (saveError) {
      setError(String(saveError));
    }
  }

  async function acceptPrivacyNotice() {
    await savePreferences({ ...preferences, privacy_notice_accepted: true });
    setPrivacyOpen(false);
  }

  async function generateSummary() {
    if (!reportId) {
      setError("请先结束一次学习会话，再生成 AI 总结。");
      return;
    }
    await runAction(() => api.generateAiSummary(reportId, preferences.ai_summary_tone), (summary) => {
      setLastReport((current) => (current ? { ...current, ai_summary: summary } : current));
    });
  }

  async function sendChat() {
    if (!reportId || !chatInput.trim()) return;
    const content = chatInput.trim();
    setChatInput("");
    const optimistic: ChatMessage = {
      id: Date.now(),
      report_id: reportId,
      role: "user",
      content,
      created_at: new Date().toISOString(),
    };
    setMessages((current) => [...current, optimistic]);
    await runAction(() => api.chatWithAi(reportId, content), (reply) => {
      setMessages((current) => [...current, reply]);
    });
  }

  async function deleteReport(reportIdToDelete: number) {
    if (!window.confirm("确定删除这条日报记录吗？今日学习总时长不会被清零。")) return;
    await runAction(() => api.deleteDailyReport(reportIdToDelete), () => {
      if (lastReport?.id === reportIdToDelete) {
        setLastReport(null);
        setMessages([]);
      }
    });
  }

  return (
    <main className="min-h-screen bg-paper text-ink">
      <header className="flex items-center justify-between border-b border-line bg-white/60 px-6 py-4">
        <div className="flex items-center gap-3">
          <div className="grid h-10 w-10 place-items-center rounded-lg bg-tomato text-white">
            <Brain size={20} />
          </div>
          <div>
            <h1 className="text-xl font-semibold">StudyPulse</h1>
            <p className="text-sm text-ink/60">{todayLabel()}</p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="rounded-md border border-line bg-white px-3 py-2 text-sm text-ink/70">
            {statusLabel(dashboard?.session_status)}
          </span>
          <button
            className="secondary-button"
            onClick={() => {
              setHistoryOpen(true);
              loadReports();
            }}
          >
            <History size={17} />
            历史日报
          </button>
          <button className="icon-button" onClick={() => setSettingsOpen(true)} aria-label="打开设置">
            <Settings size={18} />
          </button>
        </div>
      </header>

      <div className="grid gap-5 p-6 xl:grid-cols-[1.25fr_0.75fr]">
        <section className="space-y-5">
          <div className="grid gap-4 md:grid-cols-3">
            <StatCard
              label="今日学习"
              value={formatDuration(dashboard?.today_study_seconds ?? 0)}
              hint={isStudying ? "当前会话正在记录" : "开始后自动累计"}
              icon={<Clock3 size={18} />}
            />
            <StatCard label="专注度" value={`${focusScore}`} hint={focusTone} icon={<Activity size={18} />} />
            <StatCard
              label="键鼠活跃"
              value={`${(dashboard?.keyboard_count ?? 0) + (dashboard?.mouse_count ?? 0)}`}
              hint={`键盘 ${dashboard?.keyboard_count ?? 0} / 鼠标 ${dashboard?.mouse_count ?? 0}`}
              icon={<TimerReset size={18} />}
            />
          </div>

          <section className="rounded-lg border border-line bg-white/80 p-5 shadow-panel">
            <div className="flex flex-wrap items-center justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.16em] text-ink/50">Session</p>
                <h2 className="mt-2 text-4xl font-semibold">{formatDuration(dashboard?.current_session_seconds ?? 0)}</h2>
                <p className="mt-2 max-w-2xl truncate text-sm text-ink/60">
                  {dashboard?.current_app ?? "未开始"} / {dashboard?.current_window_title ?? "暂无窗口记录"}
                </p>
                <button
                  className="mt-3 inline-flex items-center gap-2 text-sm font-semibold text-moss"
                  onClick={() => setPrivacyOpen(true)}
                >
                  <ShieldCheck size={16} />
                  查看隐私说明
                </button>
              </div>
              <div className="flex gap-2">
                <button
                  className="primary-button"
                  disabled={busy || isStudying}
                  onClick={() => runAction(api.startSession)}
                >
                  <Play size={17} />
                  开始学习
                </button>
                <button
                  className="danger-button"
                  disabled={busy || !isStudying}
                  onClick={() => runAction(api.stopSession, setLastReport)}
                >
                  <Square size={16} />
                  结束学习
                </button>
              </div>
            </div>
          </section>

          <section className="grid gap-5 lg:grid-cols-[0.75fr_1fr]">
            <div className="rounded-lg border border-line bg-white/80 p-5 shadow-panel">
              <div className="flex items-start justify-between">
                <div>
                  <p className="text-xs font-semibold uppercase tracking-[0.16em] text-ink/50">Pomodoro</p>
                  <h2 className="mt-2 text-5xl font-semibold">
                    {formatDuration(dashboard?.pomodoro.remaining_seconds ?? pomodoroMinutes * 60)}
                  </h2>
                  <p className="mt-2 text-sm text-ink/60">完成 {dashboard?.pomodoro.completed_count ?? 0} 个番茄钟</p>
                </div>
                <Coffee className="text-tomato" size={24} />
              </div>

              <div className="mt-5 grid grid-cols-3 gap-2">
                {[25, 40, 50].map((minutes) => (
                  <button
                    className={pomodoroMinutes === minutes ? "primary-button justify-center" : "secondary-button justify-center"}
                    key={minutes}
                    onClick={() => savePreferences({ ...preferences, default_pomodoro_minutes: minutes })}
                  >
                    {minutes}m
                  </button>
                ))}
              </div>
              <label className="field mt-3">
                <span>自定义分钟数</span>
                <input
                  min={1}
                  max={180}
                  type="number"
                  value={pomodoroMinutes}
                  onChange={(event) =>
                    savePreferences({
                      ...preferences,
                      default_pomodoro_minutes: Number(event.target.value || 25),
                    })
                  }
                />
              </label>
              <div className="mt-4 flex gap-2">
                <button className="secondary-button" onClick={() => runAction(() => api.startPomodoro(pomodoroMinutes))}>
                  <Play size={16} />
                  开始
                </button>
                <button className="secondary-button" onClick={() => runAction(api.pausePomodoro)}>
                  暂停/继续
                </button>
                <button className="icon-button" onClick={() => runAction(api.resetPomodoro)} aria-label="重置番茄钟">
                  <RefreshCw size={17} />
                </button>
              </div>
            </div>

            <div className="rounded-lg border border-line bg-white/80 p-5 shadow-panel">
              <div className="mb-4 flex items-center justify-between">
                <h2 className="font-semibold">活跃度趋势</h2>
                <span className="text-sm text-ink/50">最近采样</span>
              </div>
              <div className="h-56">
                <ResponsiveContainer width="100%" height="100%">
                  <LineChart data={activityData}>
                    <CartesianGrid stroke="#ebe5da" vertical={false} />
                    <XAxis dataKey="label" tick={{ fontSize: 12 }} />
                    <YAxis allowDecimals={false} tick={{ fontSize: 12 }} />
                    <Tooltip />
                    <Line type="monotone" dataKey="keyboard" stroke="#2f6f5e" strokeWidth={2} dot={false} />
                    <Line type="monotone" dataKey="mouse" stroke="#d94c3d" strokeWidth={2} dot={false} />
                  </LineChart>
                </ResponsiveContainer>
              </div>
            </div>
          </section>
        </section>

        <aside className="space-y-5">
          <section className="rounded-lg border border-line bg-white/80 p-5 shadow-panel">
            <div className="mb-4 flex items-center justify-between">
              <h2 className="font-semibold">常用软件排行</h2>
              <span className="text-sm text-ink/50">Top {topApps.length}</span>
            </div>
            {topApps.length ? (
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <BarChart data={topApps} layout="vertical" margin={{ left: 12, right: 16 }}>
                    <CartesianGrid stroke="#ebe5da" horizontal={false} />
                    <XAxis type="number" tickFormatter={(value) => `${Math.round(Number(value) / 60)}m`} />
                    <YAxis dataKey="app_name" type="category" width={92} tick={{ fontSize: 12 }} />
                    <Tooltip formatter={(value) => formatDuration(Number(value))} />
                    <Bar dataKey="seconds" fill="#2f6f5e" radius={[0, 4, 4, 0]} />
                  </BarChart>
                </ResponsiveContainer>
              </div>
            ) : (
              <p className="rounded-md border border-line bg-paper p-4 text-sm text-ink/60">
                开始学习并切换几个窗口后，这里会显示应用使用时长排行。
              </p>
            )}
          </section>

          <section className="rounded-lg border border-line bg-white/80 p-5 shadow-panel">
            <div className="mb-4 flex items-center justify-between">
              <div>
                <h2 className="font-semibold">AI 总结</h2>
                <p className="text-sm text-ink/60">结束学习后生成复盘反馈</p>
              </div>
              <MessageSquareText className="text-moss" size={20} />
            </div>

            <label className="field mb-3">
              <span>总结语气</span>
              <select
                className="h-10 w-full rounded-md border border-line bg-white px-3 text-sm font-normal text-ink outline-none focus:border-moss"
                value={preferences.ai_summary_tone}
                onChange={(event) =>
                  savePreferences({ ...preferences, ai_summary_tone: event.target.value as AiSummaryTone })
                }
              >
                {Object.entries(toneLabels).map(([value, label]) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </label>

            <div className="min-h-28 rounded-md border border-line bg-paper p-4 text-sm leading-6 text-ink/75">
              {aiSummary || "还没有总结。结束一次学习会话后，可以生成本地日报和 AI 反馈。"}
            </div>

            <button className="primary-button mt-4 w-full justify-center" disabled={busy || !reportId} onClick={generateSummary}>
              {busy ? "处理中..." : "生成 AI 总结"}
            </button>

            <div className="mt-4 space-y-3">
              <div className="max-h-36 space-y-2 overflow-auto pr-1">
                {messages.map((message) => (
                  <p
                    className={message.role === "user" ? "chat-bubble ml-auto bg-moss text-white" : "chat-bubble bg-paper text-ink"}
                    key={message.id}
                  >
                    {message.content}
                  </p>
                ))}
              </div>
              <div className="flex gap-2">
                <input
                  className="min-w-0 flex-1 rounded-md border border-line bg-white px-3 py-2 text-sm outline-none focus:border-moss"
                  value={chatInput}
                  onChange={(event) => setChatInput(event.target.value)}
                  placeholder="继续问问今天的状态"
                  onKeyDown={(event) => {
                    if (event.key === "Enter") sendChat();
                  }}
                />
                <button className="secondary-button" disabled={!reportId || busy} onClick={sendChat}>
                  发送
                </button>
              </div>
            </div>
          </section>
        </aside>
      </div>

      {historyOpen ? (
        <HistoryDialog
          reports={reports}
          onClose={() => setHistoryOpen(false)}
          onRefresh={loadReports}
          onDelete={deleteReport}
        />
      ) : null}
      {privacyOpen ? (
        <PrivacyDialog
          accepted={preferences.privacy_notice_accepted}
          onAccept={acceptPrivacyNotice}
          onClose={() => setPrivacyOpen(false)}
        />
      ) : null}
      {error ? <div className="fixed bottom-4 left-1/2 -translate-x-1/2 rounded-md bg-ink px-4 py-3 text-sm text-white">{error}</div> : null}
      <SettingsModal
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onShowPrivacy={() => setPrivacyOpen(true)}
        preferences={preferences}
        onSavePreferences={savePreferences}
      />
    </main>
  );
}

function HistoryDialog({
  reports,
  onClose,
  onRefresh,
  onDelete,
}: {
  reports: DailyReport[];
  onClose: () => void;
  onRefresh: () => void;
  onDelete: (reportId: number) => void;
}) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-ink/30 p-6">
      <section className="max-h-[82vh] w-full max-w-3xl overflow-hidden rounded-lg border border-line bg-paper shadow-panel">
        <header className="flex items-center justify-between border-b border-line px-5 py-4">
          <div>
            <h2 className="text-lg font-semibold">历史日报</h2>
            <p className="text-sm text-ink/60">最近 30 条本地学习记录</p>
          </div>
          <div className="flex gap-2">
            <button className="secondary-button" onClick={onRefresh}>
              刷新
            </button>
            <button className="secondary-button" onClick={onClose}>
              关闭
            </button>
          </div>
        </header>
        <div className="max-h-[64vh] space-y-3 overflow-auto p-5">
          {reports.length ? (
            reports.map((report) => (
              <article className="rounded-lg border border-line bg-white p-4" key={report.id}>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div>
                    <h3 className="font-semibold">日报 #{report.id}</h3>
                    <p className="text-sm text-ink/60">
                      {formatDateTime(report.started_at)} - {formatDateTime(report.ended_at)}
                    </p>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 text-sm">
                    <span className="rounded-md bg-paper px-2 py-1">学习 {formatDuration(report.total_seconds)}</span>
                    <span className="rounded-md bg-paper px-2 py-1">专注 {report.focus_score}</span>
                    <span className="rounded-md bg-paper px-2 py-1">番茄 {report.pomodoro_completed}</span>
                    <button
                      className="danger-button px-2 py-1 text-xs"
                      onClick={() => onDelete(report.id)}
                      type="button"
                    >
                      <Trash2 size={14} />
                      删除
                    </button>
                  </div>
                </div>
                <p className="mt-3 text-sm text-ink/70">
                  应用排行：
                  {report.app_usage.length
                    ? report.app_usage
                        .slice(0, 3)
                        .map((item) => `${item.app_name} ${formatDuration(item.seconds)}`)
                        .join("、")
                    : "暂无采样数据"}
                </p>
                {report.ai_summary ? <p className="mt-2 text-sm leading-6 text-ink/75">{report.ai_summary}</p> : null}
              </article>
            ))
          ) : (
            <p className="rounded-md border border-line bg-white p-4 text-sm text-ink/60">
              还没有历史日报。结束一次学习会话后，这里会自动出现记录。
            </p>
          )}
        </div>
      </section>
    </div>
  );
}

function PrivacyDialog({
  accepted,
  onAccept,
  onClose,
}: {
  accepted: boolean;
  onAccept: () => void;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-ink/30 p-6">
      <section className="w-full max-w-2xl rounded-lg border border-line bg-paper shadow-panel">
        <header className="border-b border-line px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="grid h-10 w-10 place-items-center rounded-lg bg-moss text-white">
              <ShieldCheck size={20} />
            </div>
            <div>
              <h2 className="text-lg font-semibold">隐私说明</h2>
              <p className="text-sm text-ink/60">第一次使用前建议先看完这段说明</p>
            </div>
          </div>
        </header>
        <div className="space-y-3 p-5 text-sm leading-6 text-ink/75">
          <p>StudyPulse 会在学习会话中记录当前前台应用、窗口标题、应用使用时长和键鼠活跃数量。</p>
          <ul className="list-disc space-y-1 pl-5">
            <li>不记录具体按键，也不记录输入内容。</li>
            <li>不记录鼠标坐标，不截图，不录屏。</li>
            <li>数据默认保存在本机 SQLite 数据库。</li>
            <li>只有主动生成 AI 总结或继续聊天时，日报摘要才会发送到你配置的 API。</li>
          </ul>
        </div>
        <footer className="flex justify-end gap-2 border-t border-line px-5 py-4">
          {accepted ? (
            <button className="secondary-button" onClick={onClose}>
              关闭
            </button>
          ) : (
            <>
              <button className="secondary-button" onClick={onClose}>
                稍后再看
              </button>
              <button className="primary-button" onClick={onAccept}>
                我知道了
              </button>
            </>
          )}
        </footer>
      </section>
    </div>
  );
}
