import type { ChatMessage, StreamPayload, ToolCallTrace } from "../types";

export function reduceAssistantMessage(
  message: ChatMessage,
  payload: StreamPayload,
): ChatMessage {
  switch (payload.kind) {
    case "think_start":
      return {
        ...message,
        thinkingActive: true,
        thinkingContent: message.thinkingContent ?? "",
      };
    case "token":
      return message.thinkingActive
        ? {
            ...message,
            thinkingContent: (message.thinkingContent ?? "") + payload.delta,
          }
        : { ...message, content: message.content + payload.delta };
    case "think_end":
      return {
        ...message,
        thinkingActive: false,
        thinkTokens: {
          prompt: payload.prompt_tokens,
          completion: payload.completion_tokens,
        },
      };
    case "tool_call":
      return {
        ...message,
        toolCalls: [
          ...(message.toolCalls ?? []),
          {
            id: payload.tool_call_id,
            toolCallId: payload.tool_call_id,
            tool_call_id: payload.tool_call_id,
            name: payload.name,
            args: payload.args,
            startedAt: Date.now(),
          },
        ],
      };
    case "tool_result":
      return {
        ...message,
        toolCalls: patchToolCall(message.toolCalls, payload.tool_call_id, payload.name, {
          result: payload.output,
        }),
      };
    case "tool_error":
      return {
        ...message,
        toolCalls: patchToolCall(message.toolCalls, payload.tool_call_id, payload.name, {
          error: payload.error,
        }),
      };
    case "parameter_error":
      return {
        ...message,
        toolCalls: patchToolCall(message.toolCalls, undefined, payload.tool, {
          error: `参数错误: ${payload.parameter} 期望 ${payload.expected}, 实际 ${payload.got}`,
        }),
      };
    case "final_answer":
      return message.content.trim().length > 0
        ? message
        : { ...message, content: payload.text };
    case "error":
      return {
        ...message,
        status: "error",
        error: `[${payload.source}] ${payload.message}`,
      };
    case "cancelled":
      return {
        ...message,
        status: "error",
        error: "已取消",
        toolCalls: finalizePendingCalls(message.toolCalls, "已取消"),
      };
    case "done":
      return {
        ...message,
        status: payload.ok ? "done" : "error",
        error: message.error ?? payload.error ?? undefined,
        elapsedMs: payload.elapsed_ms,
        toolCalls: payload.ok
          ? message.toolCalls
          : finalizePendingCalls(message.toolCalls, payload.error ?? "未返回结果"),
      };
    default:
      return message;
  }
}

function patchToolCall(
  list: ToolCallTrace[] | undefined,
  toolCallId: string | undefined,
  name: string,
  patch: Partial<ToolCallTrace>,
): ToolCallTrace[] {
  const calls = (list ?? []).slice();
  const idIndex =
    toolCallId == null ? -1 : calls.findIndex((call) => callId(call) === toolCallId);

  if (idIndex >= 0) {
    calls[idIndex] = { ...calls[idIndex], ...patch };
    return calls;
  }

  for (let i = calls.length - 1; i >= 0; i--) {
    if (calls[i].name === name && !calls[i].result && !calls[i].error) {
      calls[i] = { ...calls[i], ...patch };
      return calls;
    }
  }

  return calls;
}

function finalizePendingCalls(
  list: ToolCallTrace[] | undefined,
  reason: string,
): ToolCallTrace[] | undefined {
  if (!list || list.length === 0) return list;
  return list.map((call) =>
    call.result || call.error ? call : { ...call, error: reason },
  );
}

function callId(call: ToolCallTrace): string | undefined {
  return call.toolCallId ?? call.tool_call_id ?? call.id;
}
