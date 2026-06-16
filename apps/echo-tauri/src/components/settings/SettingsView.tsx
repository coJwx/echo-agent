import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { Copy, Network, RefreshCw, Settings } from "lucide-react";
import { api } from "../../api";
import type { AccessTokenInfo } from "../../types";

interface SettingsViewProps {
  isTauri: boolean;
}

export default function SettingsView({ isTauri }: SettingsViewProps) {
  return (
    <div className="flex min-h-0 flex-1 flex-col lg:flex-row bg-bg-base">
      <aside className="shrink-0 border-b lg:border-b-0 lg:border-r border-border-subtle bg-bg-panel px-3 py-3 lg:py-4 lg:w-56 lg:shrink-0">
        <div className="hidden lg:block mb-3 px-2 text-[13px] font-medium text-ink-muted">设置</div>
        <nav className="flex lg:flex-col gap-1 overflow-x-auto">
          {isTauri ? (
            <SettingsNavItem selected icon={<Settings className="h-4 w-4" />}>
              通用
            </SettingsNavItem>
          ) : (
            <SettingsNavItem selected icon={<Network className="h-4 w-4" />}>
              连接
            </SettingsNavItem>
          )}
        </nav>
      </aside>
      <section className="min-w-0 flex-1 overflow-y-auto">
        {isTauri ? <DesktopSettings /> : <ConnectionSettings />}
      </section>
    </div>
  );
}

function SettingsNavItem({
  selected,
  icon,
  children,
}: {
  selected: boolean;
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={
        "flex h-9 w-full items-center gap-3 rounded-lg px-3 text-left text-[14px] transition " +
        (selected
          ? "bg-white/[0.12] text-white"
          : "text-ink-primary hover:bg-white/[0.08]")
      }
    >
      <span className="flex h-5 w-5 items-center justify-center">{icon}</span>
      <span>{children}</span>
    </button>
  );
}

function DesktopSettings() {
  return (
    <div className="p-6">
      <div className="max-w-3xl">
        <h1 className="text-[22px] font-semibold text-ink-primary">通用</h1>
        <p className="mt-2 text-[14px] leading-relaxed text-ink-secondary">
          桌面端设置正在整理中。
        </p>
      </div>
    </div>
  );
}

function ConnectionSettings() {
  const [info, setInfo] = useState<AccessTokenInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [copied, setCopied] = useState(false);

  const load = async () => {
    try {
      setInfo(await api.getAccessTokenInfo());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const handleRefresh = async () => {
    setRefreshing(true);
    setCopied(false);
    try {
      setInfo(await api.refreshAccessToken());
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRefreshing(false);
    }
  };

  const targetUrl = info?.network_url || info?.local_url || "";

  return (
    <div className="px-4 py-4 lg:px-6 lg:py-6">
      <div className="flex max-w-4xl flex-col gap-4 lg:gap-5">
        <header>
          <h1 className="text-[22px] font-semibold text-ink-primary">连接</h1>
          <p className="mt-2 max-w-2xl text-[14px] leading-relaxed text-ink-secondary">
            手机在无权限页面点击扫码后，扫描这里的二维码获取 token。访问令牌会保留 7 天，也可以主动刷新。
          </p>
        </header>

        {error && (
          <div className="rounded-lg border border-accent-red/30 bg-accent-red/10 px-3 py-2 text-[13px] text-accent-red">
            {error}
          </div>
        )}

        <section className="grid gap-4 lg:gap-5 lg:grid-cols-[320px_1fr]">
          <div className="card flex min-h-[260px] lg:min-h-[360px] flex-col items-center justify-center p-4 lg:p-5">
            {info ? (
              <div
                className="rounded-lg bg-white p-4"
                dangerouslySetInnerHTML={{ __html: info.qr_svg }}
              />
            ) : (
              <div className="h-64 w-64 animate-pulse rounded-lg bg-bg-hover" />
            )}
          </div>

          <div className="card flex flex-col gap-4 p-5">
            <div>
              <div className="text-[13px] font-medium text-ink-muted">访问 token</div>
              <div className="mt-2 rounded-lg border border-border-subtle bg-bg-panel px-3 py-2 font-mono text-[13px] leading-relaxed text-ink-primary break-all">
                {info?.token || "正在加载..."}
              </div>
            </div>

            <div>
              <div className="text-[13px] font-medium text-ink-muted">访问链接</div>
              <div className="mt-2 rounded-lg border border-border-subtle bg-bg-panel px-3 py-2 font-mono text-[13px] leading-relaxed text-ink-primary break-all">
                {targetUrl || "正在加载..."}
              </div>
            </div>

            {info && (
              <div className="grid gap-3 sm:grid-cols-2">
                <Meta label="创建时间" value={formatTime(info.created_at_ms)} />
                <Meta label="过期时间" value={formatTime(info.expires_at_ms)} />
              </div>
            )}

            <div className="flex flex-wrap gap-2 pt-1">
              <button
                type="button"
                onClick={handleRefresh}
                disabled={refreshing}
                className="btn-primary inline-flex items-center gap-2 disabled:cursor-not-allowed disabled:opacity-60"
              >
                <RefreshCw className={"h-4 w-4" + (refreshing ? " animate-spin" : "")} />
                <span>{refreshing ? "刷新中" : "刷新 token"}</span>
              </button>
              <button
                type="button"
                onClick={async () => {
                  if (!targetUrl) return;
                  await navigator.clipboard.writeText(targetUrl);
                  setCopied(true);
                }}
                disabled={!targetUrl}
                className="btn-ghost inline-flex items-center gap-2 disabled:cursor-not-allowed disabled:opacity-60"
              >
                <Copy className="h-4 w-4" />
                <span>{copied ? "已复制" : "复制链接"}</span>
              </button>
            </div>
          </div>
        </section>
      </div>
    </div>
  );
}

function Meta({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border-subtle bg-bg-panel px-3 py-2">
      <div className="text-[12px] text-ink-muted">{label}</div>
      <div className="mt-1 text-[13px] text-ink-primary">{value}</div>
    </div>
  );
}

function formatTime(ms: number) {
  return new Date(ms).toLocaleString();
}
