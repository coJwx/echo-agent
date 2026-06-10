import { useCallback, useEffect, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api } from "../api";
import type {
  ChatMessage,
  HistoryMessage,
  StreamPayload,
} from "../types";
import { reduceAssistantMessage } from "./chatReducer";
import { historyToChatMessages } from "./historyMessages";

/**
 * 单会话的消息状态 + 流式回执驱动。
 *
 * 关键不变量：
 *   1. 切换 session 时，先用 `api.history()` 拉历史，渲染，然后才订阅 stream。
 *   2. 在历史拉完之前到达的 stream 事件会被**缓存**，等历史落位后**replay** 一次，
 *      再恢复正常的 apply 路径——避免在拉取窗口中"事件已到但 messages 还没
 *      准备好 assistant 占位条"的丢事件问题。
 *   3. 切换是串行的：上一个 session 的 unlisten 在 effect cleanup 同步取消，
 *      所以新 session 不会收到旧 session 的事件。
 */
export function useChatSession(sessionId: string | null) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);

  const assistantIdRef = useRef<string | null>(null);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  // 拉取窗口期内到达的事件
  const pendingEventsRef = useRef<StreamPayload[]>([]);
  // 已就绪的 apply 函数（指 historyLoaded === true）
  const liveRef = useRef(false);

  const apply = useCallback((p: StreamPayload) => {
    if (!liveRef.current) {
      pendingEventsRef.current.push(p);
      return;
    }
    const aid = assistantIdRef.current;
    if (!aid) return;
    setMessages((prev) =>
      prev.map((m) => (m.id === aid ? reduceAssistantMessage(m, p) : m)),
    );
    if (p.kind === "done" || p.kind === "cancelled") {
      setStreaming(false);
      assistantIdRef.current = null;
    }
  }, []);

  // sessionId 变化时：清空 → 订阅 → 拉历史 → 解锁
  useEffect(() => {
    // 切换前先关掉旧 unlisten，保证新 session 不会收旧 session 事件
    unlistenRef.current?.();
    unlistenRef.current = null;

    // 重置
    setMessages([]);
    setStreaming(false);
    setHistoryLoaded(false);
    setHistoryError(null);
    assistantIdRef.current = null;
    pendingEventsRef.current = [];
    liveRef.current = false;

    if (!sessionId) {
      return;
    }

    let alive = true;

    (async () => {
      // 1. 立刻订阅新 session 的 stream
      const un = await api.listenSession(sessionId, apply);
      if (!alive) {
        un();
        return;
      }
      unlistenRef.current = un;

      // 2. 拉历史
      let history: HistoryMessage[] = [];
      try {
        history = await api.history(sessionId);
      } catch (e) {
        if (!alive) return;
        setHistoryError(e instanceof Error ? e.message : String(e));
        // 失败也解锁，让用户至少能发新消息
      }
      if (!alive) return;

      // 3. 把 history 转成 ChatMessage，渲染
      if (history.length > 0) {
        setMessages(historyToChatMessages(history));
      }

      // 4. 解锁 + replay 拉取窗口期到达的事件
      setHistoryLoaded(true);
      liveRef.current = true;
      const pending = pendingEventsRef.current.splice(0);
      for (const p of pending) apply(p);
    })();

    return () => {
      alive = false;
      unlistenRef.current?.();
      unlistenRef.current = null;
      liveRef.current = false;
    };
  }, [sessionId, apply]);

  const send = useCallback(
    async (text: string) => {
      if (!sessionId || !text.trim() || streaming) return;
      const userId = crypto.randomUUID();
      const assistantId = crypto.randomUUID();
      assistantIdRef.current = assistantId;
      setMessages((prev) => [
        ...prev,
        { id: userId, role: "user", content: text, status: "done" },
        {
          id: assistantId,
          role: "assistant",
          content: "",
          status: "streaming",
        },
      ]);
      setStreaming(true);
      try {
        await api.chat(sessionId, text);
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId ? { ...m, status: "error", error: err } : m,
          ),
        );
        setStreaming(false);
        assistantIdRef.current = null;
      }
    },
    [sessionId, streaming],
  );

  return {
    messages,
    streaming,
    historyLoaded,
    historyError,
    send,
  };
}
