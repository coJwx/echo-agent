import assert from "node:assert/strict";
import { historyToChatMessages } from "../historyMessages.js";
import type { HistoryMessage } from "../../types/index.js";

const history: HistoryMessage[] = [
  {
    id: "system-0",
    role: "system",
    content: "You are an agent.",
    status: "done",
  },
  {
    id: "user-1",
    role: "user",
    content: "列出文件",
    status: "done",
  },
  {
    id: "assistant-2",
    role: "assistant",
    content: "",
    status: "done",
    tool_calls: [{ name: "list_files", args: { path: "." } }],
  },
  {
    id: "tool-3",
    role: "tool",
    content: "src\nCargo.toml",
    status: "done",
  },
  {
    id: "assistant-4",
    role: "assistant",
    content: "",
    status: "done",
    tool_calls: [{ name: "grep", args: { pattern: "tool_calls" } }],
  },
  {
    id: "tool-5",
    role: "tool",
    content: "state.rs:374",
    status: "done",
  },
  {
    id: "assistant-6",
    role: "assistant",
    content: "已检查并完成修改。",
    thinking_content: "先检查工具结果，再总结。",
    status: "done",
    think_tokens: { prompt: 12, completion: 34 },
  },
];

const messages = historyToChatMessages(history);

assert.equal(messages.length, 3);
assert.deepEqual(
  messages.map((m) => m.role),
  ["system", "user", "assistant"],
);
assert.equal(messages[0].content, "You are an agent.");
assert.equal(messages[2].content, "已检查并完成修改。");
assert.equal(messages[2].thinkingContent, "先检查工具结果，再总结。");
assert.deepEqual(messages[2].thinkTokens, { prompt: 12, completion: 34 });
assert.equal(messages[2].toolCalls?.[0].result, "src\nCargo.toml");
assert.equal(messages[2].toolCalls?.[1].name, "grep");
assert.equal(messages[2].toolCalls?.[1].result, "state.rs:374");

const idPairedMessages = historyToChatMessages([
  {
    id: "assistant-id-1",
    role: "assistant",
    content: "",
    status: "done",
    tool_calls: [{ id: "call_1", name: "list_dir", args: { path: "." } }],
  },
  {
    id: "tool-id-2",
    role: "tool",
    content: "Directory contents",
    tool_call_id: "call_1",
    name: "list_dir",
    status: "done",
  },
]);

assert.equal(idPairedMessages[0].toolCalls?.[0].result, "Directory contents");
