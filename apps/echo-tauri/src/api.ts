import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CreateSessionInput,
  DebugChatTrace,
  HistoryMessage,
  ProviderConfig,
  ProviderModelInput,
  SessionMeta,
  StreamPayload,
} from "./types";

export const api = {
  async createSession(input: CreateSessionInput): Promise<SessionMeta> {
    return invoke<SessionMeta>("agent_create", { input });
  },

  async listSessions(): Promise<SessionMeta[]> {
    return invoke<SessionMeta[]>("agent_list_sessions");
  },

  async deleteSession(sessionId: string): Promise<void> {
    return invoke<void>("agent_delete_session", { sessionId });
  },

  async getProviderConfig(): Promise<ProviderConfig> {
    return invoke<ProviderConfig>("provider_config_get");
  },

  async saveProviderModel(input: ProviderModelInput): Promise<ProviderConfig> {
    return invoke<ProviderConfig>("provider_model_save", { input });
  },

  /** 读取会话历史。切换会话时调用，渲染历史消息。 */
  async history(sessionId: string): Promise<HistoryMessage[]> {
    return invoke<HistoryMessage[]>("agent_history", { sessionId });
  },

  /** 启动流式对话。事件通过 listenSession 拿。 */
  async chat(sessionId: string, message: string): Promise<void> {
    return invoke<void>("agent_chat_stream", { sessionId, message });
  },

  /** 调试用：真实调用当前 session 的模型和工具，并返回完整事件序列与最终历史。 */
  async debugChatCollect(sessionId: string, message: string): Promise<DebugChatTrace> {
    return invoke<DebugChatTrace>("agent_debug_chat_collect", { sessionId, message });
  },

  /** 订阅一个 session 的事件流。返回 unlisten 函数。 */
  async listenSession(
    sessionId: string,
    onEvent: (payload: StreamPayload) => void,
  ): Promise<UnlistenFn> {
    const channel = `echo://agent/stream/${sessionId}`;
    return listen<StreamPayload>(channel, (e) => onEvent(e.payload));
  },
};
