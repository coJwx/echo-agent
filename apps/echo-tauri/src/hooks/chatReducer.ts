import type {
  ChatMessage,
  ChatMessageSegment,
  StreamPayload,
  ToolCallTrace,
} from "../types";

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
        segments: appendThinkingSegment(message.segments),
      };
    case "token":
      return message.thinkingActive
        ? {
            ...message,
            thinkingContent: (message.thinkingContent ?? "") + payload.delta,
            segments: appendThinkingToken(message.segments, payload.delta),
          }
        : {
            ...message,
            content: message.content + payload.delta,
            segments: appendTextToken(message.segments, payload.delta),
          };
    case "think_end":
      return {
        ...message,
        thinkingActive: false,
        thinkTokens: {
          prompt: payload.prompt_tokens,
          completion: payload.completion_tokens,
        },
        segments: patchLastThinkingSegmentTokens(
          message.segments,
          payload.prompt_tokens,
          payload.completion_tokens,
        ),
      };
    case "tool_call":
      {
        const call = {
          id: payload.tool_call_id,
          toolCallId: payload.tool_call_id,
          tool_call_id: payload.tool_call_id,
          name: payload.name,
          args: payload.args,
          startedAt: Date.now(),
        };
        return {
          ...message,
          toolCalls: [...(message.toolCalls ?? []), call],
          segments: [...(message.segments ?? []), { kind: "tool_call", call }],
        };
      }
    case "tool_result":
      return {
        ...message,
        toolCalls: patchToolCall(message.toolCalls, payload.tool_call_id, payload.name, {
          result: payload.output,
        }),
        segments: patchToolSegment(message.segments, payload.tool_call_id, payload.name, {
          result: payload.output,
        }),
      };
    case "tool_error":
      return {
        ...message,
        toolCalls: patchToolCall(message.toolCalls, payload.tool_call_id, payload.name, {
          error: payload.error,
        }),
        segments: patchToolSegment(message.segments, payload.tool_call_id, payload.name, {
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
        : {
            ...message,
            content: payload.text,
            segments: appendTextToken(message.segments, payload.text),
          };
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

function appendThinkingSegment(
  segments: ChatMessageSegment[] | undefined,
): ChatMessageSegment[] {
  return [...(segments ?? []), { kind: "thinking", content: "" }];
}

function appendThinkingToken(
  segments: ChatMessageSegment[] | undefined,
  delta: string,
): ChatMessageSegment[] {
  const next = segments?.slice() ?? [{ kind: "thinking", content: "" }];
  for (let i = next.length - 1; i >= 0; i--) {
    const segment = next[i];
    if (segment.kind === "thinking") {
      next[i] = { ...segment, content: segment.content + delta };
      return next;
    }
  }
  return [...next, { kind: "thinking", content: delta }];
}

function patchLastThinkingSegmentTokens(
  segments: ChatMessageSegment[] | undefined,
  prompt: number,
  completion: number,
): ChatMessageSegment[] | undefined {
  if (!segments?.length) return segments;
  const next = segments.slice();
  for (let i = next.length - 1; i >= 0; i--) {
    const segment = next[i];
    if (segment.kind === "thinking") {
      next[i] = { ...segment, tokens: { prompt, completion } };
      return next;
    }
  }
  return next;
}

function appendTextToken(
  segments: ChatMessageSegment[] | undefined,
  delta: string,
): ChatMessageSegment[] {
  const next = segments?.slice() ?? [];
  const last = next[next.length - 1];
  if (last?.kind === "text") {
    next[next.length - 1] = { ...last, content: last.content + delta };
    return next;
  }
  return [...next, { kind: "text", content: delta }];
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

function patchToolSegment(
  list: ChatMessageSegment[] | undefined,
  toolCallId: string | undefined,
  name: string,
  patch: Partial<ToolCallTrace>,
): ChatMessageSegment[] | undefined {
  if (!list?.length) return list;
  const next = list.slice();
  const idIndex =
    toolCallId == null
      ? -1
      : next.findIndex(
          (segment) => segment.kind === "tool_call" && callId(segment.call) === toolCallId,
        );

  if (idIndex >= 0 && next[idIndex].kind === "tool_call") {
    next[idIndex] = {
      kind: "tool_call",
      call: { ...next[idIndex].call, ...patch },
    };
    return next;
  }

  for (let i = next.length - 1; i >= 0; i--) {
    const segment = next[i];
    if (
      segment.kind === "tool_call" &&
      segment.call.name === name &&
      !segment.call.result &&
      !segment.call.error
    ) {
      next[i] = {
        kind: "tool_call",
        call: { ...segment.call, ...patch },
      };
      return next;
    }
  }

  return next;
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
