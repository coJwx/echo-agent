import { useEffect, useState } from "react";
import { api } from "./api";
import Sidebar, { type TabKey } from "./components/Sidebar";
import AccessDeniedView from "./components/AccessDeniedView";
import ChatView from "./components/chat/ChatView";
import NewSessionDialog from "./components/chat/NewSessionDialog";
import ComingSoon from "./components/ComingSoon";
import ProviderConfigView from "./components/provider/ProviderConfigView";
import SettingsView from "./components/settings/SettingsView";
import type { SessionMeta } from "./types";

export default function App() {
  const isTauri = api.isTauriRuntime();
  const [authorized, setAuthorized] = useState(
    () => isTauri || api.hasWebAuthToken(),
  );
  const [tab, setTab] = useState<TabKey>("chat");
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeSession, setActiveSession] = useState<string | null>(null);
  const [showNewSession, setShowNewSession] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  const refreshSessions = async () => {
    try {
      const list = await api.listSessions();
      setSessions(list);
      setSessionError(null);
      setActiveSession((current) => current ?? list[0]?.id ?? null);
    } catch (e) {
      if (!isTauri && api.isAuthError(e)) {
        api.clearWebAuthToken();
        setAuthorized(false);
        setSessions([]);
        setActiveSession(null);
        setSessionError(null);
        return;
      }
      setSessionError(e instanceof Error ? e.message : String(e));
    }
  };

  useEffect(() => {
    if (authorized) {
      refreshSessions();
    }
  }, [authorized]);

  if (!authorized) {
    return (
      <AccessDeniedView
        onSubmitToken={(token) => {
          api.setWebAuthToken(token);
          setAuthorized(true);
        }}
      />
    );
  }

  const handleNewChat = () => {
    setTab("chat");
    setShowNewSession(true);
  };

  const handleSelectSession = (id: string) => {
    setTab("chat");
    setActiveSession(id);
  };

  const handleDeleteSession = async (id: string) => {
    await api.deleteSession(id);
    setActiveSession((current) => (current === id ? null : current));
    await refreshSessions();
  };

  const handleSessionUpdated = (meta: SessionMeta) => {
    setSessions((current) =>
      current.map((session) => (session.id === meta.id ? meta : session)),
    );
    setActiveSession(meta.id);
  };

  const handleCreatedSession = async (meta: SessionMeta) => {
    setShowNewSession(false);
    await refreshSessions();
    setActiveSession(meta.id);
  };

  const currentSession =
    sessions.find((session) => session.id === activeSession) ?? null;

  return (
    <div className="flex h-full w-full bg-bg-base">
      {/* Mobile backdrop — tap to close sidebar */}
      {!sidebarCollapsed && (
        <div
          className="fixed inset-0 z-30 bg-black/60 lg:hidden"
          onClick={() => setSidebarCollapsed(true)}
        />
      )}
      <Sidebar
        active={tab}
        collapsed={sidebarCollapsed}
        sessions={sessions}
        activeSession={activeSession}
        sessionError={sessionError}
        onChange={setTab}
        onNewChat={handleNewChat}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
        mobileOnClose={() => setSidebarCollapsed(true)}
      />
      <main className="flex-1 min-w-0 flex flex-col">
        {tab === "chat" && (
          <ChatView
            active={activeSession}
            current={currentSession}
            sidebarCollapsed={sidebarCollapsed}
            onToggleSidebar={() =>
              setSidebarCollapsed((collapsed) => !collapsed)
            }
            onSessionUpdated={handleSessionUpdated}
          />
        )}
        {tab === "mcp" && (
          <ComingSoon
            title="MCP 配置"
            description="动态接入 filesystem / database / web-search 等 MCP 服务。计划 v0.2 上线。"
            hint="底层 echo_agent::advanced::McpManager 已就绪，前端 UI 待补。"
          />
        )}
        {tab === "skill" && (
          <ComingSoon
            title="Skill 技能"
            description="管理内置技能（FileSystem / Shell）与 agentskills.io 外部技能。"
            hint="底层 SkillRegistry 已就绪，前端 UI 待补。"
          />
        )}
        {tab === "provider" && <ProviderConfigView />}
        {tab === "settings" && <SettingsView isTauri={isTauri} />}
      </main>
      {showNewSession && (
        <NewSessionDialog
          onCancel={() => setShowNewSession(false)}
          onCreated={handleCreatedSession}
        />
      )}
    </div>
  );
}
