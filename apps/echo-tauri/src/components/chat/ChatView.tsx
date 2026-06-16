import { useEffect, useMemo, useState } from "react";
import {
  Braces,
  ChevronLeft,
  Command,
  Menu,
  MoreVertical,
  PanelRightOpen,
  RotateCcw,
  X,
  type LucideIcon,
} from "lucide-react";
import { api } from "../../api";
import type { SessionMeta } from "../../types";
import { useChatSession } from "../../hooks/useChatSession";
import MessageList from "./MessageList";
import MessageInput from "./MessageInput";

interface ChatViewProps {
  active: string | null;
  current: SessionMeta | null;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  onSessionUpdated: (meta: SessionMeta) => void;
}

export default function ChatView({
  active,
  current,
  sidebarCollapsed,
  onToggleSidebar,
  onSessionUpdated,
}: ChatViewProps) {
  const [showInspector, setShowInspector] = useState(false);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [modelError, setModelError] = useState<string | null>(null);
  const [updatingModel, setUpdatingModel] = useState(false);

  const { messages, streaming, historyLoaded, historyError, send } =
    useChatSession(active);
  const toolbarItems: Array<{ icon: LucideIcon; label: string }> = [
    { icon: Braces, label: "上下文" },
    { icon: RotateCcw, label: "重新生成" },
    { icon: Command, label: "命令" },
    { icon: MoreVertical, label: "更多" },
  ];
  const selectableModels = useMemo(() => {
    const values = new Set(modelOptions);
    if (current?.model) values.add(current.model);
    return Array.from(values);
  }, [current?.model, modelOptions]);

  useEffect(() => {
    let cancelled = false;
    api
      .getProviderConfig()
      .then((config) => {
        if (!cancelled) {
          setModelOptions(config.models.map((model) => model.name));
        }
      })
      .catch((e) => {
        if (!cancelled) setModelError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function updateModel(model: string) {
    if (!active || !current || model === current.model || streaming) return;
    setUpdatingModel(true);
    setModelError(null);
    try {
      const next = await api.updateSessionModel(active, { model });
      onSessionUpdated(next);
    } catch (e) {
      setModelError(e instanceof Error ? e.message : String(e));
    } finally {
      setUpdatingModel(false);
    }
  }

  return (
    <div className="relative flex min-h-0 flex-1 bg-[#111111]">
      <section className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 items-center justify-between border-b border-[#242424] bg-[#171717] px-3 lg:px-5">
          <div className="flex min-w-0 items-center gap-2 lg:gap-3">
            <button
              type="button"
              onClick={onToggleSidebar}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-[#2f2f2f] bg-[#222] text-ink-secondary transition hover:border-[#4a4a4a] hover:bg-[#2a2a2a] hover:text-ink-primary"
              title={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
              aria-label={sidebarCollapsed ? "展开侧边栏" : "收起侧边栏"}
              aria-expanded={!sidebarCollapsed}
            >
              {sidebarCollapsed ? (
                <Menu className="h-4 w-4" strokeWidth={2} />
              ) : (
                <ChevronLeft className="h-4 w-4" strokeWidth={2} />
              )}
            </button>
            <div className="min-w-0 text-[15px] font-semibold text-ink-primary">
              <div className="truncate">{current?.title ?? "选择或新建一个对话"}</div>
            </div>
          </div>
          <div className="flex items-center gap-2 lg:gap-3">
            {current && (
              <label className="hidden md:flex items-center gap-2 text-[12px] text-ink-secondary">
                <span className="hidden lg:inline">模型</span>
                <select
                  className="h-8 max-w-36 lg:max-w-52 rounded-lg border border-[#2f2f2f] bg-[#222] px-2 text-[12px] text-ink-primary outline-none transition hover:border-[#4a4a4a] focus:border-brand disabled:opacity-60"
                  value={current.model}
                  onChange={(event) => updateModel(event.target.value)}
                  disabled={streaming || updatingModel || selectableModels.length === 0}
                  title={modelError ? `模型列表加载失败: ${modelError}` : "切换模型"}
                >
                  {selectableModels.map((model) => (
                    <option key={model} value={model}>
                      {model}
                    </option>
                  ))}
                </select>
              </label>
            )}
            {streaming && (
              <span className="flex items-center gap-2 text-[12px] text-brand">
                <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-brand" />
                生成中
              </span>
            )}
            <div className="flex items-center gap-1 lg:gap-2">
              {toolbarItems.slice(0, 2).map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    key={item.label}
                    type="button"
                    className="hidden md:flex h-8 w-8 items-center justify-center rounded-lg border border-[#2f2f2f] bg-[#222] text-xs text-ink-secondary transition hover:border-[#4a4a4a] hover:bg-[#2a2a2a] hover:text-ink-primary"
                    title={item.label}
                    aria-label={item.label}
                  >
                    <Icon className="h-4 w-4" strokeWidth={2} />
                  </button>
                );
              })}
              {toolbarItems.slice(2).map((item) => {
                const Icon = item.icon;
                return (
                  <button
                    key={item.label}
                    type="button"
                    className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#2f2f2f] bg-[#222] text-xs text-ink-secondary transition hover:border-[#4a4a4a] hover:bg-[#2a2a2a] hover:text-ink-primary"
                    title={item.label}
                    aria-label={item.label}
                  >
                    <Icon className="h-4 w-4" strokeWidth={2} />
                  </button>
                );
              })}
              {!showInspector && (
                <button
                  type="button"
                  onClick={() => setShowInspector((visible) => !visible)}
                  className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#2f2f2f] bg-[#222] text-ink-secondary transition hover:border-[#4a4a4a] hover:bg-[#2a2a2a] hover:text-ink-primary"
                  title={showInspector ? "隐藏属性面板" : "显示属性面板"}
                  aria-label={showInspector ? "隐藏属性面板" : "显示属性面板"}
                >
                  <PanelRightOpen className="h-4 w-4" strokeWidth={2} />
                </button>
              )}
            </div>
          </div>
        </header>
        {active && !historyLoaded && (
          <div className="flex items-center gap-2 px-5 pt-2 text-[12px] text-ink-secondary">
            <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ink-muted" />
            加载历史记录…
          </div>
        )}
        {historyError && (
          <div className="mx-5 mt-3 rounded border border-accent-amber/30 bg-accent-amber/10 px-3 py-2 text-[12px] text-accent-amber">
            历史加载失败（仍可继续对话）: {historyError}
          </div>
        )}
        <MessageList messages={messages} placeholder={!active} />
        <MessageInput
          disabled={!active || streaming || !historyLoaded}
          onSend={send}
        />
      </section>
      {showInspector && (
        <>
          {/* Mobile overlay backdrop */}
          <div
            className="fixed inset-0 z-40 bg-black/60 lg:hidden"
            onClick={() => setShowInspector(false)}
          />
          <aside className="w-80 shrink-0 border-l border-[#242424] bg-[#171717] p-4 max-lg:fixed max-lg:inset-y-0 max-lg:right-0 max-lg:z-50 max-lg:w-full max-lg:max-w-md max-lg:shadow-2xl max-lg:shadow-black/50">
            <div className="flex items-center justify-between">
              <h2 className="text-[14px] font-semibold text-ink-primary">属性</h2>
              <button
                type="button"
                onClick={() => setShowInspector(false)}
                className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#2f2f2f] bg-[#222] text-xs text-ink-secondary transition hover:border-[#4a4a4a] hover:bg-[#2a2a2a] hover:text-ink-primary"
                title="收起属性面板"
                aria-label="收起属性面板"
              >
                <X className="h-4 w-4" strokeWidth={2} />
              </button>
            </div>
          <div className="mt-3 space-y-3">
            <MetaField label="智能体 ID" value={current?.id.slice(0, 12) ?? "agent_12345"} />
            <MetaField label="名称" value={current?.title ?? "数据分析助手"} />
            <MetaField label="模型" value={current?.model ?? "gpt-4o"} />
            <div>
              <div className="mb-1.5 text-[12px] text-ink-secondary">温度</div>
              <div className="flex items-center gap-3">
                <div className="h-1 flex-1 rounded-full bg-[#2a2a2a]">
                  <div className="h-1 w-1/3 rounded-full bg-brand" />
                </div>
                <span className="rounded bg-[#262626] px-2 py-1 text-[12px] text-ink-primary">
                  0.2
                </span>
              </div>
            </div>
            <div>
              <div className="mb-1.5 text-[12px] text-ink-secondary">最大 Tokens</div>
              <div className="flex items-center gap-3">
                <div className="h-1 flex-1 rounded-full bg-[#2a2a2a]">
                  <div className="h-1 w-3/4 rounded-full bg-brand" />
                </div>
                <span className="rounded bg-[#262626] px-2 py-1 text-[12px] text-ink-primary">
                  4096
                </span>
              </div>
            </div>
            <div>
              <div className="mb-1.5 text-[12px] text-ink-secondary">系统提示词</div>
              <div className="min-h-20 rounded-lg border border-[#2f2f2f] bg-[#202020] p-2.5 text-[13px] leading-[1.45] text-ink-primary">
                {current?.system_prompt ||
                  "你是一位专业的数据分析助手，擅长使用 SQL 进行查询和可视化分析。"}
              </div>
            </div>
            <MetaField
              label="创建时间"
              value={current ? formatDate(current.created_at_ms) : "2024-06-01 10:20:30"}
            />
            <MetaField
              label="连接状态"
              value={streaming ? "生成中" : current ? "就绪" : "未选择"}
            />
          </div>
        </aside>
        </>
      )}
    </div>
  );
}

function MetaField({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="mb-1 text-[12px] text-ink-secondary">{label}</div>
      <div className="rounded-lg border border-[#2f2f2f] bg-[#202020] px-3 py-1.5 text-[13px] text-ink-primary">
        {value}
      </div>
    </div>
  );
}

function formatDate(ms: number) {
  return new Date(ms).toLocaleString("zh-CN", {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}
