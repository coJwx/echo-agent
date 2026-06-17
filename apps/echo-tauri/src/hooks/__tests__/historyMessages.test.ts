import assert from "node:assert/strict";
import { historyToChatMessages } from "../historyMessages.js";
import type { ChatTurn } from "../../types/index.js";

const history: ChatTurn[] = [
  {
    id: "system-0",
    role: "system",
    status: "done",
    segments: [{ kind: "text", content: "You are an agent." }],
  },
  {
    id: "user-1",
    role: "user",
    status: "done",
    segments: [{ kind: "text", content: "分析项目" }],
  },
  {
    id: "assistant-2",
    role: "assistant",
    status: "done",
    elapsed_ms: 1200,
    segments: [
      { kind: "thinking", content: "需要先确认项目结构。" },
      {
        kind: "tool_call",
        call: {
          id: "call_find",
          name: "find",
          args: { path: "." },
          result: "Cargo.toml\nsrc",
        },
      },
      { kind: "text", content: "这是一个 Rust 项目。" },
      { kind: "thinking", content: "现在可以总结。" },
      { kind: "text", content: "可以开始实现。" },
    ],
  },
];

const messages = historyToChatMessages(history);

assert.equal(messages.length, 3);
assert.deepEqual(
  messages.map((m) => m.role),
  ["system", "user", "assistant"],
);
assert.equal(messages[2].content, "这是一个 Rust 项目。\n\n可以开始实现。");
assert.equal(
  messages[2].thinkingContent,
  "需要先确认项目结构。\n\n现在可以总结。",
);
assert.equal(messages[2].toolCalls?.[0].toolCallId, "call_find");
assert.equal(messages[2].toolCalls?.[0].result, "Cargo.toml\nsrc");
assert.deepEqual(
  messages[2].segments?.map((segment) => segment.kind),
  ["thinking", "tool_call", "text", "thinking", "text"],
);
assert.equal(messages[2].elapsedMs, 1200);
