import type { SessionMeta } from "../../types";
import { MessageSquare, Plus, X } from "lucide-react";

interface Props {
  sessions: SessionMeta[];
  active: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
  onDelete: (id: string) => void;
}

export default function SessionList({
  sessions,
  active,
  onSelect,
  onNew,
  onDelete,
}: Props) {
  return (
    <aside className="flex w-64 shrink-0 flex-col border-r border-border-strong bg-[#0d121a]">
      <div className="border-b border-border-subtle px-3 py-3">
        <div className="flex items-center justify-between">
          <div className="text-[14px] font-medium text-ink-primary">对话记录</div>
          <button
            type="button"
            onClick={onNew}
            className="flex h-7 w-7 items-center justify-center rounded-md border border-border-subtle text-ink-muted transition hover:border-brand hover:text-ink-primary"
            title="新对话"
            aria-label="新对话"
          >
            <Plus className="h-4 w-4" strokeWidth={2} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-2.5">
        {sessions.length === 0 ? (
          <div className="mt-6 rounded-lg border border-dashed border-border-subtle px-3 py-4 text-center text-[12px] leading-snug text-ink-secondary">
            尚无会话。点击左侧新对话开始。
          </div>
        ) : (
          sessions.map((s) => {
            const selected = s.id === active;
            return (
              <div
                key={s.id}
                className={
                  "group mb-1.5 flex cursor-pointer items-center gap-2.5 rounded-lg border px-2.5 py-2 transition " +
                  (selected
                    ? "border-brand/50 bg-brand-soft text-ink-primary"
                    : "border-transparent text-ink-secondary hover:border-border-subtle hover:bg-bg-hover/60 hover:text-ink-primary")
                }
                onClick={() => onSelect(s.id)}
              >
                <MessageSquare className="h-5 w-5 shrink-0 text-ink-muted" strokeWidth={1.8} />
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[14px] font-medium">{s.title}</div>
                  <div className="mt-0.5 truncate text-[12px] text-ink-secondary">
                    {formatRelativeTime(s.updated_at_ms)}
                  </div>
                </div>
                <button
                  type="button"
                  className="flex h-6 w-6 items-center justify-center rounded text-ink-muted opacity-0 transition hover:bg-accent-red/10 hover:text-accent-red group-hover:opacity-100"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (confirm(`删除会话 "${s.title}"？`)) onDelete(s.id);
                  }}
                  title="删除"
                  aria-label="删除"
                >
                  <X className="h-3.5 w-3.5" strokeWidth={2} />
                </button>
              </div>
            );
          })
        )}
      </div>
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
