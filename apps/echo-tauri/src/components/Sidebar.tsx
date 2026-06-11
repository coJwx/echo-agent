import type { SessionMeta } from "../types";
import {
  Command,
  PencilLine,
  Plug,
  Puzzle,
  Search,
  Settings,
  X,
  type LucideIcon,
} from "lucide-react";

export type TabKey = "chat" | "mcp" | "skill" | "provider" | "settings";
type NavKey = Exclude<TabKey, "chat">;

interface SidebarProps {
  active: TabKey;
  collapsed: boolean;
  sessions: SessionMeta[];
  activeSession: string | null;
  sessionError: string | null;
  onChange: (tab: TabKey) => void;
  onNewChat: () => void;
  onSelectSession: (id: string) => void;
  onDeleteSession: (id: string) => void;
}

interface NavItem {
  key: NavKey;
  label: string;
  icon: LucideIcon;
}

const ITEMS: NavItem[] = [
  { key: "mcp", label: "MCP", icon: Command },
  { key: "skill", label: "Skill", icon: Puzzle },
  { key: "provider", label: "Provider", icon: Plug },
  { key: "settings", label: "设置", icon: Settings },
];

export default function Sidebar({
  active,
  collapsed,
  sessions,
  activeSession,
  sessionError,
  onChange,
  onNewChat,
  onSelectSession,
  onDeleteSession,
}: SidebarProps) {
  return (
    <aside
      className={
        "flex shrink-0 flex-col overflow-hidden bg-[#1c1f26] transition-[width] duration-200 ease-out " +
        (collapsed ? "w-0 border-r-0" : "w-80 border-r border-[#2a2a2a]")
      }
      aria-hidden={collapsed}
    >
      {!collapsed && (
        <>
          <div className="space-y-1 px-3 py-2">
            <button
              type="button"
              onClick={onNewChat}
              className="flex h-9 w-full items-center gap-3 rounded-lg px-3 text-left text-[15px] font-medium text-ink-primary transition hover:bg-white/[0.08]"
            >
              <PencilLine className="h-4 w-4 shrink-0" strokeWidth={2} />
              <span>新对话</span>
            </button>
            <button
              type="button"
              className="flex h-9 w-full items-center gap-3 rounded-lg px-3 text-left text-[15px] font-medium text-ink-primary transition hover:bg-white/[0.08]"
              title="搜索"
              aria-label="搜索"
            >
              <Search className="h-4 w-4 shrink-0" strokeWidth={2} />
              <span>搜索</span>
            </button>
          </div>

          <div className="relative min-h-0 flex-1">
            <div className="h-full overflow-y-auto px-2.5 pb-12 pt-4">
              {sessionError && (
                <div className="mb-2 rounded-lg border border-accent-red/30 bg-accent-red/10 px-3 py-2 text-[12px] leading-snug text-accent-red">
                  会话加载失败: {sessionError}
                </div>
              )}
              {sessions.length === 0 ? (
                <div className="rounded-lg border border-dashed border-[#343434] px-3 py-4 text-center text-[12px] leading-snug text-ink-secondary">
                  尚无会话
                </div>
              ) : (
                <div className="space-y-0.5">
                  {sessions.map((session) => {
                    const selected =
                      active === "chat" && session.id === activeSession;
                    return (
                      <div
                        key={session.id}
                        className={
                          "group relative flex h-9 cursor-pointer items-center gap-2 rounded-lg px-3 transition " +
                          (selected
                            ? "bg-white/[0.12] text-ink-primary"
                            : "text-ink-primary hover:bg-white/[0.08]")
                        }
                        onClick={() => onSelectSession(session.id)}
                      >
                        <div className="min-w-0 flex-1 truncate text-[14px] font-medium">
                          {session.title}
                        </div>
                        <span className="max-w-16 shrink-0 text-right text-[13px] text-ink-primary/80 transition group-hover:opacity-0">
                          {formatRelativeTime(session.updated_at_ms)}
                        </span>
                        <button
                          type="button"
                          className="absolute right-2 top-1/2 flex h-6 w-6 -translate-y-1/2 shrink-0 items-center justify-center rounded text-ink-muted opacity-0 transition hover:bg-accent-red/10 hover:text-accent-red group-hover:opacity-100"
                          onClick={(e) => {
                            e.stopPropagation();
                            if (confirm(`删除会话 "${session.title}"？`)) {
                              onDeleteSession(session.id);
                            }
                          }}
                          title="删除"
                          aria-label="删除会话"
                        >
                          <X className="h-3.5 w-3.5" strokeWidth={2} />
                        </button>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
            <div className="pointer-events-none absolute inset-x-0 top-0 h-4 bg-gradient-to-b from-[#1c1f26] via-[#1c1f26]/85 to-[#1c1f26]/0" />
            <div className="pointer-events-none absolute inset-x-0 bottom-0 h-3 bg-gradient-to-b from-[#1c1f26]/0 via-[#1c1f26]/100 to-[#1c1f26]" />
          </div>

          <nav className="space-y-1 px-2.5 pb-2">
            {ITEMS.map((item) => {
              const selected = item.key === active;
              const Icon = item.icon;
              return (
                <button
                  key={item.key}
                  onClick={() => onChange(item.key)}
                  className={
                    "flex h-9 w-full items-center gap-3 rounded-lg px-3 text-[14px] transition " +
                    (selected
                      ? "bg-white/[0.12] text-white"
                      : "text-ink-primary hover:bg-white/[0.08]")
                  }
                >
                  <span className="flex h-5 w-5 items-center justify-center text-base">
                    <Icon className="h-4 w-4" strokeWidth={2} />
                  </span>
                  <span>{item.label}</span>
                </button>
              );
            })}
          </nav>
        </>
      )}
    </aside>
  );
}

function formatRelativeTime(ms: number) {
  const diff = Date.now() - ms;
  const minute = 60 * 1000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < hour) return `${Math.max(1, Math.round(diff / minute))} 分钟前`;
  if (diff < day) return `${Math.round(diff / hour)} 小时前`;
  return `${Math.round(diff / day)} 天前`;
}
