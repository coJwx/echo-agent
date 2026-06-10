import assert from "node:assert/strict";
import { reduceAssistantMessage } from "../chatReducer.js";
import type { ChatMessage } from "../../types/index.js";

function base(): ChatMessage {
  return {
    id: "assistant-1",
    role: "assistant",
    content: "",
    status: "streaming",
  };
}

let message = reduceAssistantMessage(base(), { kind: "think_start" });
message = reduceAssistantMessage(message, { kind: "token", delta: "think" });
message = reduceAssistantMessage(message, {
  kind: "think_end",
  prompt_tokens: 1,
  completion_tokens: 2,
});
message = reduceAssistantMessage(message, { kind: "token", delta: "answer" });

assert.equal(message.thinkingContent, "think");
assert.equal(message.content, "answer");
assert.deepEqual(message.thinkTokens, { prompt: 1, completion: 2 });

message = reduceAssistantMessage(base(), {
  kind: "tool_call",
  tool_call_id: "call_1",
  name: "list_dir",
  args: { path: "." },
});
message = reduceAssistantMessage(message, {
  kind: "tool_result",
  tool_call_id: "call_1",
  name: "list_dir",
  output: "Directory contents",
});

assert.equal(message.toolCalls?.[0].toolCallId, "call_1");
assert.equal(message.toolCalls?.[0].result, "Directory contents");

message = reduceAssistantMessage(base(), { kind: "token", delta: "hello" });
message = reduceAssistantMessage(message, { kind: "final_answer", text: "hello" });

assert.equal(message.content, "hello");
