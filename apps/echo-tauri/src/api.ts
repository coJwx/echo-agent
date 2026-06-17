import type {
  AccessTokenInfo,
  ChatTurn,
  CreateSessionInput,
  DebugChatTrace,
  ProviderConfig,
  ProviderModelInput,
  SessionMeta,
  StreamPayload,
  UpdateSessionModelInput,
} from "./types";

type UnlistenFn = () => void;

class ApiAuthError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ApiAuthError";
  }
}

interface ApiTransport {
  createSession(input: CreateSessionInput): Promise<SessionMeta>;
  listSessions(): Promise<SessionMeta[]>;
  deleteSession(sessionId: string): Promise<void>;
  updateSessionModel(
    sessionId: string,
    input: UpdateSessionModelInput,
  ): Promise<SessionMeta>;
  getProviderConfig(): Promise<ProviderConfig>;
  saveProviderModel(input: ProviderModelInput): Promise<ProviderConfig>;
  history(sessionId: string): Promise<ChatTurn[]>;
  chat(sessionId: string, message: string): Promise<void>;
  debugChatCollect(sessionId: string, message: string): Promise<DebugChatTrace>;
  listenSession(
    sessionId: string,
    onEvent: (payload: StreamPayload) => void,
  ): Promise<UnlistenFn>;
  getAccessTokenInfo(): Promise<AccessTokenInfo>;
  refreshAccessToken(): Promise<AccessTokenInfo>;
}

let transportPromise: Promise<ApiTransport> | null = null;

export const api = {
  async createSession(input: CreateSessionInput): Promise<SessionMeta> {
    return (await getTransport()).createSession(input);
  },

  async listSessions(): Promise<SessionMeta[]> {
    return (await getTransport()).listSessions();
  },

  async deleteSession(sessionId: string): Promise<void> {
    return (await getTransport()).deleteSession(sessionId);
  },

  async updateSessionModel(
    sessionId: string,
    input: UpdateSessionModelInput,
  ): Promise<SessionMeta> {
    return (await getTransport()).updateSessionModel(sessionId, input);
  },

  async getProviderConfig(): Promise<ProviderConfig> {
    return (await getTransport()).getProviderConfig();
  },

  async saveProviderModel(input: ProviderModelInput): Promise<ProviderConfig> {
    return (await getTransport()).saveProviderModel(input);
  },

  /** 读取会话历史。切换会话时调用，渲染历史消息。 */
  async history(sessionId: string): Promise<ChatTurn[]> {
    return (await getTransport()).history(sessionId);
  },

  /** 启动流式对话。事件通过 listenSession 拿。 */
  async chat(sessionId: string, message: string): Promise<void> {
    return (await getTransport()).chat(sessionId, message);
  },

  /** 调试用：真实调用当前 session 的模型和工具，并返回完整事件序列与最终历史。 */
  async debugChatCollect(
    sessionId: string,
    message: string,
  ): Promise<DebugChatTrace> {
    return (await getTransport()).debugChatCollect(sessionId, message);
  },

  /** 订阅一个 session 的事件流。返回 unlisten 函数。 */
  async listenSession(
    sessionId: string,
    onEvent: (payload: StreamPayload) => void,
  ): Promise<UnlistenFn> {
    return (await getTransport()).listenSession(sessionId, onEvent);
  },

  isTauriRuntime,

  hasWebAuthToken(): boolean {
    return Boolean(readWebAuthToken());
  },

  setWebAuthToken(token: string): void {
    writeWebAuthToken(token);
    transportPromise = null;
  },

  clearWebAuthToken(): void {
    clearWebAuthToken();
    transportPromise = null;
  },

  isAuthError(error: unknown): boolean {
    return error instanceof ApiAuthError;
  },

  async getAccessTokenInfo(): Promise<AccessTokenInfo> {
    return (await getTransport()).getAccessTokenInfo();
  },

  async refreshAccessToken(): Promise<AccessTokenInfo> {
    const info = await (await getTransport()).refreshAccessToken();
    writeWebAuthToken(info.token);
    transportPromise = null;
    return info;
  },
};

function getTransport(): Promise<ApiTransport> {
  transportPromise ??= createTransport();
  return transportPromise;
}

async function createTransport(): Promise<ApiTransport> {
  return isTauriRuntime() ? createTauriTransport() : createWebTransport();
}

function isTauriRuntime(): boolean {
  if (typeof window === "undefined") return false;
  const candidate = window as unknown as {
    __TAURI_INTERNALS__?: unknown;
    __TAURI__?: unknown;
  };
  return Boolean(candidate.__TAURI_INTERNALS__ || candidate.__TAURI__);
}

async function createTauriTransport(): Promise<ApiTransport> {
  const [{ invoke }, { listen }] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/api/event"),
  ]);

  return {
    createSession(input) {
      return invoke<SessionMeta>("agent_create", { input });
    },
    listSessions() {
      return invoke<SessionMeta[]>("agent_list_sessions");
    },
    deleteSession(sessionId) {
      return invoke<void>("agent_delete_session", { sessionId });
    },
    updateSessionModel(sessionId, input) {
      return invoke<SessionMeta>("agent_update_model", { sessionId, input });
    },
    getProviderConfig() {
      return invoke<ProviderConfig>("provider_config_get");
    },
    saveProviderModel(input) {
      return invoke<ProviderConfig>("provider_model_save", { input });
    },
    history(sessionId) {
      return invoke<ChatTurn[]>("agent_history", { sessionId });
    },
    chat(sessionId, message) {
      return invoke<void>("agent_chat_stream", { sessionId, message });
    },
    debugChatCollect(sessionId, message) {
      return invoke<DebugChatTrace>("agent_debug_chat_collect", {
        sessionId,
        message,
      });
    },
    listenSession(sessionId, onEvent) {
      const channel = `echo://agent/stream/${sessionId}`;
      return listen<StreamPayload>(channel, (e) => onEvent(e.payload));
    },
    async getAccessTokenInfo() {
      throw new Error("Access token settings are only available in web mode");
    },
    async refreshAccessToken() {
      throw new Error("Access token settings are only available in web mode");
    },
  };
}

function createWebTransport(): ApiTransport {
  const baseUrl = webHttpBaseUrl();
  const token = webAuthToken();
  const sockets = new Map<string, SessionSocket>();

  return {
    createSession(input) {
      return request<SessionMeta>(baseUrl, token, "/api/sessions", {
        method: "POST",
        body: JSON.stringify(input),
      });
    },
    listSessions() {
      return request<SessionMeta[]>(baseUrl, token, "/api/sessions");
    },
    async deleteSession(sessionId) {
      await request<void>(
        baseUrl,
        token,
        `/api/sessions/${encodeURIComponent(sessionId)}`,
        {
          method: "DELETE",
        },
      );
    },
    updateSessionModel(sessionId, input) {
      return request<SessionMeta>(
        baseUrl,
        token,
        `/api/sessions/${encodeURIComponent(sessionId)}/model`,
        {
          method: "POST",
          body: JSON.stringify(input),
        },
      );
    },
    getProviderConfig() {
      return request<ProviderConfig>(baseUrl, token, "/api/provider/config");
    },
    saveProviderModel(input) {
      return request<ProviderConfig>(baseUrl, token, "/api/provider/models", {
        method: "POST",
        body: JSON.stringify(input),
      });
    },
    history(sessionId) {
      return request<ChatTurn[]>(
        baseUrl,
        token,
        `/api/sessions/${encodeURIComponent(sessionId)}/history`,
      );
    },
    async chat(sessionId, message) {
      const socket = ensureSessionSocket(sockets, baseUrl, token, sessionId);
      await socket.open;
      socket.ws.send(JSON.stringify({ type: "chat", message }));
    },
    async debugChatCollect() {
      throw new Error("debugChatCollect is only available inside the Tauri app");
    },
    async listenSession(sessionId, onEvent) {
      const socket = ensureSessionSocket(sockets, baseUrl, token, sessionId);
      socket.callbacks.add(onEvent);
      await socket.open;
      return () => {
        socket.callbacks.delete(onEvent);
        if (socket.callbacks.size === 0) {
          socket.ws.close();
          sockets.delete(sessionId);
        }
      };
    },
    getAccessTokenInfo() {
      return request<AccessTokenInfo>(baseUrl, token, "/api/access-token");
    },
    refreshAccessToken() {
      return request<AccessTokenInfo>(baseUrl, token, "/api/access-token/refresh", {
        method: "POST",
      });
    },
  };
}

interface SessionSocket {
  ws: WebSocket;
  open: Promise<void>;
  callbacks: Set<(payload: StreamPayload) => void>;
}

function ensureSessionSocket(
  sockets: Map<string, SessionSocket>,
  baseUrl: string,
  token: string,
  sessionId: string,
): SessionSocket {
  const existing = sockets.get(sessionId);
  if (
    existing &&
    (existing.ws.readyState === WebSocket.OPEN ||
      existing.ws.readyState === WebSocket.CONNECTING)
  ) {
    return existing;
  }

  const ws = new WebSocket(webSocketUrl(baseUrl, token, sessionId));
  const callbacks = new Set<(payload: StreamPayload) => void>();
  const open = new Promise<void>((resolve, reject) => {
    ws.addEventListener("open", () => resolve(), { once: true });
    ws.addEventListener("error", () => reject(new Error("WebSocket connection failed")), {
      once: true,
    });
  });

  const socket = { ws, open, callbacks };
  sockets.set(sessionId, socket);

  ws.addEventListener("message", (event) => {
    const payload = JSON.parse(String(event.data)) as StreamPayload;
    for (const callback of callbacks) callback(payload);
  });
  ws.addEventListener("close", () => {
    if (sockets.get(sessionId) === socket) sockets.delete(sessionId);
  });

  return socket;
}

async function request<T>(
  baseUrl: string,
  token: string,
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(new URL(path, baseUrl), {
    ...init,
    credentials: "include",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
      ...init.headers,
    },
  });

  if (!response.ok) {
    const text = await response.text();
    let message = text || response.statusText;
    if (text) {
      try {
        const body = JSON.parse(text) as { error?: string };
        message = body.error || message;
      } catch {
        // Keep the plain-text body.
      }
    }
    if (response.status === 401 || response.status === 403) {
      throw new ApiAuthError(message || "Unauthorized");
    }
    throw new Error(message);
  }

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}

function webHttpBaseUrl(): string {
  const env = (import.meta as unknown as {
    env?: Record<string, string | undefined>;
  }).env;
  return env?.VITE_ECHO_HTTP_BASE_URL || window.location.origin;
}

function webAuthToken(): string {
  const fromUrl = new URLSearchParams(window.location.search).get("token");
  if (fromUrl) {
    writeWebAuthToken(fromUrl);
    const url = new URL(window.location.href);
    url.searchParams.delete("token");
    window.history.replaceState({}, "", url);
    return fromUrl;
  }

  const fromCookie = readWebAuthToken();
  if (fromCookie) return fromCookie;
  throw new Error("Missing access token. Open the URL printed by echo-http-server.");
}

function readWebAuthToken(): string | null {
  if (typeof document === "undefined") return null;
  const fromCookie = document.cookie
    .split(";")
    .map((part) => part.trim())
    .find((part) => part.startsWith("echo_http_token="))
    ?.split("=")[1];

  return fromCookie ? decodeURIComponent(fromCookie) : null;
}

function writeWebAuthToken(token: string) {
  document.cookie = `echo_http_token=${encodeURIComponent(token)}; path=/; SameSite=Lax`;
}

function clearWebAuthToken() {
  document.cookie = "echo_http_token=; path=/; max-age=0; SameSite=Lax";
}

function webSocketUrl(baseUrl: string, token: string, sessionId: string): string {
  const url = new URL(
    `/api/sessions/${encodeURIComponent(sessionId)}/stream`,
    baseUrl,
  );
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  url.searchParams.set("token", token);
  return url.toString();
}
