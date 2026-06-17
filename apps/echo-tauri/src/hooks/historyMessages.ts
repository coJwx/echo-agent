import type { ChatMessage, ChatMessageSegment, ChatTurn, ToolCallTrace } from "../types/index.js";

export function historyToChatMessages(history: ChatTurn[]): ChatMessage[] {
  return history
    .map(turnToChatMessage)
    .filter((message) => isVisibleMessage(message));
}

function turnToChatMessage(turn: ChatTurn): ChatMessage {
  const segments = normalizeSegments(turn.segments);
  const content = segments
    .filter((segment): segment is Extract<ChatMessageSegment, { kind: "text" }> =>
      segment.kind === "text",
    )
    .map((segment) => segment.content.trim())
    .filter(Boolean)
    .join("\n\n");
  const thinkingContent = segments
    .filter((segment): segment is Extract<ChatMessageSegment, { kind: "thinking" }> =>
      segment.kind === "thinking",
    )
    .map((segment) => segment.content.trim())
    .filter(Boolean)
    .join("\n\n");
  const toolCalls = segments
    .filter((segment): segment is Extract<ChatMessageSegment, { kind: "tool_call" }> =>
      segment.kind === "tool_call",
    )
    .map((segment) => segment.call);

  return {
    id: turn.id,
    role: turn.role,
    content,
    thinkingContent: thinkingContent || undefined,
    toolCalls: toolCalls.length ? toolCalls : undefined,
    segments,
    status: turn.status === "error" ? "error" : turn.status === "streaming" ? "streaming" : "done",
    elapsedMs: turn.elapsedMs ?? turn.elapsed_ms,
  };
}

function normalizeSegments(segments: ChatMessageSegment[]): ChatMessageSegment[] {
  return segments.map((segment) => {
    if (segment.kind !== "tool_call") return segment;
    return {
      kind: "tool_call",
      call: normalizeToolCall(segment.call),
    };
  });
}

function normalizeToolCall(call: ToolCallTrace): ToolCallTrace {
  return {
    ...call,
    toolCallId: call.toolCallId ?? call.tool_call_id ?? call.id,
  };
}

function isVisibleMessage(message: ChatMessage): boolean {
  return (
    message.content.trim().length > 0 ||
    Boolean(message.thinkingContent?.trim()) ||
    Boolean(message.toolCalls?.length) ||
    Boolean(message.error) ||
    message.status === "streaming"
  );
}
