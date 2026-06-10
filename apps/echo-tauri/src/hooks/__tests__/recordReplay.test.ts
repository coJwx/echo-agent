import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { reduceAssistantMessage } from "../chatReducer.js";
import type { ChatMessage, StreamPayload } from "../../types/index.js";

const recordPath = resolve("src-tauri/test-record.jsonl");
const events = readFileSync(recordPath, "utf8")
  .trim()
  .split(/\r?\n/)
  .map((line) => JSON.parse(line) as RecordLine)
  .filter((record) => record.phase === "frontend.listen.received.raw")
  .map((record) => record.payload.stream_payload)
  .filter(isStreamPayload);

let message: ChatMessage = {
  id: "assistant-replay",
  role: "assistant",
  content: "",
  status: "streaming",
};

for (const event of events) {
  message = reduceAssistantMessage(message, event);
}

assert.equal(message.status, "done");
assert.equal(message.toolCalls?.length, 1);
assert.ok((message.toolCalls?.[0].result?.length ?? 0) > 0);
assert.ok(message.content.length > 0);

interface RecordLine {
  phase: string;
  payload: {
    stream_payload?: StreamPayload;
  };
}

function isStreamPayload(payload: StreamPayload | undefined): payload is StreamPayload {
  return payload != null;
}
