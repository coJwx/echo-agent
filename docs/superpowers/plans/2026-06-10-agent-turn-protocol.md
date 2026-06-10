# Agent Turn Protocol Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make one AI turn behave consistently across core agent execution, Tauri streaming/history, and frontend rendering.

**Architecture:** Add stable tool call identity and non-empty tool observations at the core event/history boundary, expose those fields through Tauri, and consume events with a pure frontend reducer. Keep existing event names where possible and use compatibility fallbacks for old history.

**Tech Stack:** Rust workspace (`echo-core`, `echo-agent`, `echo-tauri`), Tauri event channel, React/TypeScript hooks and reducer tests.

---

## File Map

- Modify `echo-core/src/agent/mod.rs`: add `tool_call_id` to tool-related `AgentEvent` variants and helpers.
- Modify `src/agent/react/run/processor.rs`: preserve streaming tool call IDs and keep reasoning delta behavior.
- Modify `src/agent/react/run/execution.rs`: align non-streaming tool observation creation with stream path.
- Modify `src/agent/react/run/stream_channel.rs`: emit tool events with IDs and use shared observation rules.
- Modify `apps/echo-tauri/src-tauri/src/events.rs`: serialize tool IDs to frontend payloads.
- Modify `apps/echo-tauri/src-tauri/src/state.rs`: include tool IDs and tool message metadata in history.
- Modify `apps/echo-tauri/src-tauri/src/commands.rs`: strengthen real trace assertions.
- Modify `apps/echo-tauri/src/types/index.ts`: add `toolCallId` / `tool_call_id` fields.
- Create `apps/echo-tauri/src/hooks/chatReducer.ts`: pure stream-to-message reducer.
- Modify `apps/echo-tauri/src/hooks/useChatSession.ts`: delegate event application to reducer.
- Modify `apps/echo-tauri/src/hooks/historyMessages.ts`: pair history tool results by ID.
- Add or modify frontend tests under `apps/echo-tauri/src/hooks/__tests__/`.

## Task 1: Core Event Identity And Tool Observations

**Files:**
- Modify: `echo-core/src/agent/mod.rs`
- Modify: `src/agent/react/run/processor.rs`
- Modify: `src/agent/react/run/execution.rs`
- Modify: `src/agent/react/run/stream_channel.rs`

- [ ] **Step 1: Write failing tests for tool call IDs in stream processor**

Add tests in `src/agent/react/run/processor.rs`:

```rust
#[test]
fn streaming_tool_call_preserves_provider_id() {
    let mut map = HashMap::new();
    map.insert(
        0,
        (
            "call_provider_1".to_string(),
            "list_dir".to_string(),
            r#"{"path":"."}"#.to_string(),
        ),
    );

    let (msg_calls, steps) = build_tool_calls_from_map(&map);

    assert_eq!(msg_calls[0].id, "call_provider_1");
    assert_eq!(steps[0].0, "call_provider_1");
    assert_eq!(steps[0].1, "list_dir");
}
```

- [ ] **Step 2: Run the processor test**

Run:

```powershell
cargo test -p echo_agent streaming_tool_call_preserves_provider_id
```

Expected before implementation: fail if IDs are dropped or regenerated incorrectly.

- [ ] **Step 3: Extend `AgentEvent` tool variants**

Change `echo-core/src/agent/mod.rs` variants to include IDs:

```rust
ToolCall {
    tool_call_id: String,
    name: String,
    args: Value,
},
ToolResult {
    tool_call_id: String,
    name: String,
    output: String,
},
ToolError {
    tool_call_id: String,
    name: String,
    error: String,
},
ToolStream {
    tool_call_id: String,
    name: String,
    event: crate::tools::ToolStreamEvent,
},
```

Update every compile error from constructing these variants by passing the existing provider/tool-call ID already available in each loop.

- [ ] **Step 4: Add a helper for non-empty tool observations**

In `src/agent/react/run/execution.rs`, add a small helper near tool execution helpers:

```rust
pub(crate) fn tool_observation_text(
    tool_name: &str,
    result: &crate::tools::ToolResult,
) -> String {
    if result.success {
        if result.output.is_empty() {
            format!("[Tool {tool_name} completed successfully with empty output]")
        } else {
            result.output.clone()
        }
    } else {
        let msg = result
            .error
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                if result.output.is_empty() {
                    "Tool returned failure without an error message".to_string()
                } else {
                    result.output.clone()
                }
            });
        format!("[Tool execution failed] {tool_name}: {msg}")
    }
}
```

- [ ] **Step 5: Use the helper in stream and non-stream paths**

In `execution.rs` and `stream_channel.rs`, use `tool_observation_text(tool_name, &result)` before pushing `Message::tool_result(...)` or returning softened tool feedback. Ensure the string is never empty.

- [ ] **Step 6: Run targeted backend tests**

Run:

```powershell
cargo test -p echo_agent streaming_tool_call_preserves_provider_id
cargo test -p echo_agent strip_reasoning_content_removes_provider_specific_field
```

Expected: both pass.

## Task 2: Tauri Payload And History Projection

**Files:**
- Modify: `apps/echo-tauri/src-tauri/src/events.rs`
- Modify: `apps/echo-tauri/src-tauri/src/state.rs`
- Modify: `apps/echo-tauri/src-tauri/src/commands.rs`

- [ ] **Step 1: Update `StreamPayload` tool variants**

Change tool payload variants in `events.rs`:

```rust
ToolCall {
    tool_call_id: String,
    name: String,
    args: Value,
},
ToolResult {
    tool_call_id: String,
    name: String,
    output: String,
},
ToolError {
    tool_call_id: String,
    name: String,
    error: String,
},
ToolStream {
    tool_call_id: String,
    name: String,
    payload: Value,
},
```

Map from `AgentEvent` by copying `tool_call_id`.

- [ ] **Step 2: Extend history structs**

In `state.rs`, add:

```rust
pub tool_call_id: Option<String>,
pub name: Option<String>,
```

to `HistoryMessage`, and add:

```rust
pub id: String,
```

to `HistoryToolCall`.

- [ ] **Step 3: Populate history IDs**

In `history_message`, for assistant tool calls, set `HistoryToolCall.id = c.id.clone()`. For `Role::Tool`, set `tool_call_id = m.tool_call_id.clone()` and `name = m.name.clone()`.

- [ ] **Step 4: Strengthen real trace assertions**

In `record_real_frontend_backend_stream_trace`, add assertions that collect tool call IDs and result IDs from raw payloads:

```rust
assert_eq!(tool_call_ids.len(), 1, "expected exactly one tool call id");
assert_eq!(tool_result_ids, tool_call_ids, "tool result ids must match calls");
assert!(tool_result_outputs.iter().all(|s| !s.is_empty()));
```

- [ ] **Step 5: Run the real trace test**

Run:

```powershell
cargo test --manifest-path apps\echo-tauri\src-tauri\Cargo.toml record_real_frontend_backend_stream_trace -- --ignored --nocapture
```

Expected: pass, and `apps\echo-tauri\src-tauri\test-record.jsonl` contains one `tool_call`, one matching `tool_result`, non-empty output, and `done.ok == true`.

## Task 3: Frontend Pure Reducer

**Files:**
- Create: `apps/echo-tauri/src/hooks/chatReducer.ts`
- Modify: `apps/echo-tauri/src/hooks/useChatSession.ts`
- Modify: `apps/echo-tauri/src/types/index.ts`
- Add: `apps/echo-tauri/src/hooks/__tests__/chatReducer.test.ts`

- [ ] **Step 1: Update TypeScript stream types**

In `types/index.ts`, add IDs:

```ts
| { kind: "tool_call"; tool_call_id: string; name: string; args: unknown }
| { kind: "tool_result"; tool_call_id: string; name: string; output: string }
| { kind: "tool_error"; tool_call_id: string; name: string; error: string }
```

Extend `ToolCallTrace`:

```ts
export interface ToolCallTrace {
  id?: string;
  toolCallId?: string;
  tool_call_id?: string;
  name: string;
  args: unknown;
  result?: string;
  error?: string;
  startedAt?: number;
}
```

- [ ] **Step 2: Write reducer tests**

Create `chatReducer.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { reduceAssistantMessage } from "../chatReducer";
import type { ChatMessage } from "../../types";

function base(): ChatMessage {
  return { id: "a1", role: "assistant", content: "", status: "streaming" };
}

describe("reduceAssistantMessage", () => {
  it("routes tokens to thinking until think_end", () => {
    let msg = reduceAssistantMessage(base(), { kind: "think_start" });
    msg = reduceAssistantMessage(msg, { kind: "token", delta: "think" });
    msg = reduceAssistantMessage(msg, {
      kind: "think_end",
      prompt_tokens: 1,
      completion_tokens: 2,
    });
    msg = reduceAssistantMessage(msg, { kind: "token", delta: "answer" });

    expect(msg.thinkingContent).toBe("think");
    expect(msg.content).toBe("answer");
  });

  it("pairs tool results by tool_call_id", () => {
    let msg = reduceAssistantMessage(base(), {
      kind: "tool_call",
      tool_call_id: "call_1",
      name: "list_dir",
      args: { path: "." },
    });
    msg = reduceAssistantMessage(msg, {
      kind: "tool_result",
      tool_call_id: "call_1",
      name: "list_dir",
      output: "Directory contents",
    });

    expect(msg.toolCalls?.[0].result).toBe("Directory contents");
  });

  it("does not duplicate final_answer after streamed content", () => {
    let msg = reduceAssistantMessage(base(), { kind: "token", delta: "hello" });
    msg = reduceAssistantMessage(msg, { kind: "final_answer", text: "hello" });
    expect(msg.content).toBe("hello");
  });
});
```

- [ ] **Step 3: Run reducer tests and confirm failure**

Run:

```powershell
npm --prefix apps\echo-tauri test -- --run chatReducer
```

Expected before implementation: fail because `chatReducer.ts` does not exist.

- [ ] **Step 4: Implement `chatReducer.ts`**

Create `chatReducer.ts` with:

```ts
import type { ChatMessage, StreamPayload, ToolCallTrace } from "../types";

export function reduceAssistantMessage(m: ChatMessage, p: StreamPayload): ChatMessage {
  switch (p.kind) {
    case "think_start":
      return { ...m, thinkingActive: true, thinkingContent: m.thinkingContent ?? "" };
    case "token":
      return m.thinkingActive
        ? { ...m, thinkingContent: (m.thinkingContent ?? "") + p.delta }
        : { ...m, content: m.content + p.delta };
    case "think_end":
      return {
        ...m,
        thinkingActive: false,
        thinkTokens: { prompt: p.prompt_tokens, completion: p.completion_tokens },
      };
    case "tool_call":
      return {
        ...m,
        toolCalls: [
          ...(m.toolCalls ?? []),
          {
            id: p.tool_call_id,
            toolCallId: p.tool_call_id,
            name: p.name,
            args: p.args,
            startedAt: Date.now(),
          },
        ],
      };
    case "tool_result":
      return {
        ...m,
        toolCalls: patchToolCall(m.toolCalls, p.tool_call_id, p.name, { result: p.output }),
      };
    case "tool_error":
      return {
        ...m,
        toolCalls: patchToolCall(m.toolCalls, p.tool_call_id, p.name, { error: p.error }),
      };
    case "final_answer":
      return m.content.trim().length > 0 ? m : { ...m, content: p.text };
    case "done":
      return { ...m, status: p.ok ? "done" : "error", error: m.error ?? p.error ?? undefined, elapsedMs: p.elapsed_ms };
    case "cancelled":
      return { ...m, status: "error", error: "已取消" };
    case "error":
      return { ...m, status: "error", error: `[${p.source}] ${p.message}` };
    default:
      return m;
  }
}

function patchToolCall(
  list: ToolCallTrace[] | undefined,
  id: string | undefined,
  name: string,
  patch: Partial<ToolCallTrace>,
): ToolCallTrace[] {
  const arr = (list ?? []).slice();
  const idx = arr.findIndex((c) => callId(c) === id);
  if (idx >= 0) {
    arr[idx] = { ...arr[idx], ...patch };
    return arr;
  }
  for (let i = arr.length - 1; i >= 0; i--) {
    if (arr[i].name === name && !arr[i].result && !arr[i].error) {
      arr[i] = { ...arr[i], ...patch };
      return arr;
    }
  }
  return arr;
}

function callId(call: ToolCallTrace): string | undefined {
  return call.toolCallId ?? call.tool_call_id ?? call.id;
}
```

- [ ] **Step 5: Delegate hook reducer**

In `useChatSession.ts`, import `reduceAssistantMessage` and replace local `reduceMessage` usage:

```ts
import { reduceAssistantMessage } from "./chatReducer";
```

Then map events with:

```ts
prev.map((m) => (m.id === aid ? reduceAssistantMessage(m, p) : m))
```

Remove duplicated reducer helpers from `useChatSession.ts`.

- [ ] **Step 6: Run frontend reducer tests**

Run:

```powershell
npm --prefix apps\echo-tauri test -- --run chatReducer
```

Expected: pass.

## Task 4: History Pairing By ID

**Files:**
- Modify: `apps/echo-tauri/src/hooks/historyMessages.ts`
- Add or modify: `apps/echo-tauri/src/hooks/__tests__/historyMessages.test.ts`

- [ ] **Step 1: Add history pairing test**

Add a test where assistant tool call and later tool result share `tool_call_id`:

```ts
it("attaches tool history by tool_call_id", () => {
  const messages = historyToChatMessages([
    {
      id: "assistant-1",
      role: "assistant",
      content: "",
      status: "done",
      tool_calls: [{ id: "call_1", name: "list_dir", args: { path: "." } }],
    },
    {
      id: "tool-2",
      role: "tool",
      content: "Directory contents",
      status: "done",
      tool_call_id: "call_1",
      name: "list_dir",
    },
  ]);

  expect(messages[0].toolCalls?.[0].result).toBe("Directory contents");
});
```

- [ ] **Step 2: Implement ID-aware attachment**

In `historyMessages.ts`, when a role `tool` message has `tool_call_id`, find a previous assistant call whose `id`, `toolCallId`, or `tool_call_id` matches it. Use name/last-pending only as fallback.

- [ ] **Step 3: Run history tests**

Run:

```powershell
npm --prefix apps\echo-tauri test -- --run historyMessages
```

Expected: pass.

## Task 5: End-To-End Trace And Replay Verification

**Files:**
- Modify: `apps/echo-tauri/src-tauri/src/commands.rs`
- Add: `apps/echo-tauri/src/hooks/__tests__/recordReplay.test.ts`
- Generated: `apps/echo-tauri/src-tauri/test-record.jsonl`

- [ ] **Step 1: Run real backend trace**

Run:

```powershell
cargo test --manifest-path apps\echo-tauri\src-tauri\Cargo.toml record_real_frontend_backend_stream_trace -- --ignored --nocapture
```

Expected: pass and rewrite `apps\echo-tauri\src-tauri\test-record.jsonl`.

- [ ] **Step 2: Add frontend replay test**

Create a test that reads `test-record.jsonl`, extracts `frontend.listen.received.raw.stream_payload`, and feeds events into `reduceAssistantMessage`.

Expected final state:

```ts
expect(message.status).toBe("done");
expect(message.toolCalls).toHaveLength(1);
expect(message.toolCalls?.[0].result?.length).toBeGreaterThan(0);
expect(message.content.length).toBeGreaterThan(0);
```

- [ ] **Step 3: Run replay test**

Run:

```powershell
npm --prefix apps\echo-tauri test -- --run recordReplay
```

Expected: pass.

- [ ] **Step 4: Final targeted verification**

Run:

```powershell
cargo test -p echo_tools normalize_current_dir_keeps_current_dir
cargo test --manifest-path apps\echo-tauri\src-tauri\Cargo.toml record_real_frontend_backend_stream_trace -- --ignored --nocapture
npm --prefix apps\echo-tauri test -- --run chatReducer historyMessages recordReplay
```

Expected: all pass. Do not run `npm run build` or `cargo check`.

## Self-Review

- Spec coverage: core stream/non-stream behavior, Tauri payload/history, frontend reducer, history replay, and real trace verification are covered.
- Placeholder scan: passed; there are no open-ended implementation steps, and each task has concrete files and commands.
- Type consistency: frontend uses `tool_call_id` from Rust payload and supports `toolCallId` compatibility in UI state.
