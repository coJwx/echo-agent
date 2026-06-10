import type { ChatMessage, HistoryMessage, ToolCallTrace } from "../types/index.js";

type RenderableHistoryMessage = Omit<HistoryMessage, "role"> & {
  role: "user" | "assistant" | "system";
};

export function historyToChatMessages(history: HistoryMessage[]): ChatMessage[] {
  const messages: ChatMessage[] = [];
  let pendingToolOnlyAssistant: ChatMessage | null = null;

  for (const h of history) {
    if (h.role === "tool") {
      if (pendingToolOnlyAssistant) {
        pendingToolOnlyAssistant = attachToolResultToMessage(
          pendingToolOnlyAssistant,
          h.content,
          h.tool_call_id ?? undefined,
          h.name ?? undefined,
        );
      } else {
        attachToolResult(
          messages,
          h.content,
          h.tool_call_id ?? undefined,
          h.name ?? undefined,
        );
      }
      continue;
    }

    if (!isRenderableHistoryMessage(h)) {
      continue;
    }

    const message = historyToChatMessage(h);
    if (isVisibleMessage(message)) {
      if (isToolOnlyAssistant(message)) {
        pendingToolOnlyAssistant = mergeToolOnlyAssistant(pendingToolOnlyAssistant, message);
        continue;
      }
      if (pendingToolOnlyAssistant) {
        if (message.role === "assistant") {
          messages.push(mergeAssistantMessage(pendingToolOnlyAssistant, message));
          pendingToolOnlyAssistant = null;
          continue;
        }
        messages.push(pendingToolOnlyAssistant);
        pendingToolOnlyAssistant = null;
      }
      messages.push(message);
    }
  }

  if (pendingToolOnlyAssistant) {
    messages.push(pendingToolOnlyAssistant);
  }

  return messages;
}

function isRenderableHistoryMessage(h: HistoryMessage): h is RenderableHistoryMessage {
  return h.role === "user" || h.role === "assistant" || h.role === "system";
}

function historyToChatMessage(h: RenderableHistoryMessage): ChatMessage {
  return {
    id: h.id,
    role: h.role,
    content: h.content,
    thinkingContent: h.thinkingContent ?? h.thinking_content,
    toolCalls: normalizeToolCalls(h.toolCalls ?? h.tool_calls),
    thinkTokens: h.thinkTokens ?? h.think_tokens,
    status: h.status === "error" ? "error" : "done",
    elapsedMs: h.elapsedMs ?? h.elapsed_ms,
  };
}

function normalizeToolCalls(calls: ToolCallTrace[] | undefined): ToolCallTrace[] | undefined {
  if (!calls || calls.length === 0) return undefined;
  return calls.map((call) => ({
    ...call,
    toolCallId: call.toolCallId ?? call.tool_call_id ?? call.id,
  }));
}

function attachToolResult(
  messages: ChatMessage[],
  result: string,
  toolCallId?: string,
  name?: string,
) {
  if (!result) return;

  for (let i = messages.length - 1; i >= 0; i--) {
    const message = messages[i];
    if (message.role !== "assistant" || !message.toolCalls?.length) continue;

    messages[i] = attachToolResultToMessage(message, result, toolCallId, name);
    return;
  }
}

function attachToolResultToMessage(
  message: ChatMessage,
  result: string,
  toolCallId?: string,
  name?: string,
): ChatMessage {
  if (!result || !message.toolCalls?.length) return message;
  const calls = message.toolCalls.slice();
  const targetIndex = findPendingToolCall(calls, toolCallId, name);
  calls[targetIndex] = { ...calls[targetIndex], result };
  return { ...message, toolCalls: calls };
}

function mergeToolOnlyAssistant(
  pending: ChatMessage | null,
  message: ChatMessage,
): ChatMessage {
  if (!pending) return message;
  return mergeAssistantMessage(pending, message);
}

function mergeAssistantMessage(base: ChatMessage, next: ChatMessage): ChatMessage {
  return {
    ...next,
    id: base.id,
    toolCalls: [...(base.toolCalls ?? []), ...(next.toolCalls ?? [])],
    thinkingContent: next.thinkingContent ?? base.thinkingContent,
    thinkTokens: next.thinkTokens ?? base.thinkTokens,
    elapsedMs: next.elapsedMs ?? base.elapsedMs,
  };
}

function isToolOnlyAssistant(message: ChatMessage): boolean {
  return (
    message.role === "assistant" &&
    message.content.trim().length === 0 &&
    Boolean(message.toolCalls?.length) &&
    !message.error
  );
}

function findPendingToolCall(
  calls: ToolCallTrace[],
  toolCallId?: string,
  name?: string,
): number {
  if (toolCallId) {
    const byId = calls.findIndex((call) => callId(call) === toolCallId);
    if (byId >= 0) return byId;
  }
  if (name) {
    for (let i = calls.length - 1; i >= 0; i--) {
      if (calls[i].name === name && !calls[i].result && !calls[i].error) {
        return i;
      }
    }
  }
  for (let i = calls.length - 1; i >= 0; i--) {
    if (!calls[i].result && !calls[i].error) return i;
  }
  return calls.length - 1;
}

function callId(call: ToolCallTrace): string | undefined {
  return call.toolCallId ?? call.tool_call_id ?? call.id;
}

function isVisibleMessage(message: ChatMessage): boolean {
  return (
    message.content.trim().length > 0 ||
    Boolean(message.toolCalls?.length) ||
    Boolean(message.error) ||
    message.status === "streaming"
  );
}
