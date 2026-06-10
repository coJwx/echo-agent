# Agent Turn Protocol Design

## Goal

Unify the full AI task/chat flow across `echo_agent`, the Tauri bridge, persisted session history, and the React UI so thinking text, answer text, tool calls, tool results, and final answers are rendered and replayed exactly once.

## Current Problems

- `Token` currently carries both reasoning and answer text, so the receiver must infer destination from `ThinkStart`/`ThinkEnd`.
- `ToolCall` and `ToolResult` are matched by tool name in the desktop UI, which is unsafe for repeated or parallel calls.
- Streaming and non-streaming tool execution paths handle `ToolResult::error` differently.
- Session history stores assistant/tool messages in LLM format, while the UI reconstructs a rendered turn by guessing adjacency.
- Tests previously checked isolated functions but did not verify the real event order from model stream to UI state.

## Protocol

The system will treat one user message as one assistant turn. A turn may include:

1. `think_start`
2. zero or more `token` events routed to thinking while thinking is active
3. `think_end`
4. zero or more `token` events routed to answer content
5. zero or more `tool_call` events
6. matching `tool_result` or `tool_error` events
7. optional later thinking/content phases after tool results
8. `final_answer`
9. `done`

The existing `ThinkStart`, `Token`, and `ThinkEnd` events remain valid. The required semantic rule is: while the frontend turn reducer is in `thinking` phase, `token.delta` appends to `thinkingContent`; otherwise it appends to `content`.

Tool events must carry a stable `tool_call_id`. Name-only matching remains a compatibility fallback for older records but must not be the primary path. Tool results sent back to the LLM must never be empty. If a tool returns `ToolResult::error`, the model receives a non-empty observation containing the error text.

## Backend Design

`echo-core/src/agent/mod.rs` will extend tool events with optional or required `tool_call_id` fields while keeping compatibility constructors or conversions where needed.

`src/agent/react/run/processor.rs` will preserve provider tool IDs from streaming chunks and allocate a stable fallback ID only when the provider omits one. Reasoning chunks that arrive as cumulative text will still be converted to deltas before emitting `Token`.

`src/agent/react/run/execution.rs` and `src/agent/react/run/stream_channel.rs` will share the same rules for turning tool execution outcomes into:

- `AgentEvent::ToolResult` or `AgentEvent::ToolError`
- `Message::tool_result(tool_call_id, name, observation)`
- trace events
- audit records

The shared rule is: success uses `result.output`; failure uses `result.error` or a generated error string. Empty observations are replaced with a descriptive fallback before being written to history.

## Tauri Design

`apps/echo-tauri/src-tauri/src/events.rs` will expose `tool_call_id` on `tool_call`, `tool_result`, `tool_error`, and `tool_stream` payloads.

`apps/echo-tauri/src-tauri/src/state.rs` will expose history records with enough data to restore UI state without guessing:

- assistant tool calls include `id`, `name`, and `args`
- tool messages include `tool_call_id`, `name`, and `content`
- existing role/content fields remain for compatibility

The debug trace command/test will continue writing raw records to `apps/echo-tauri/src-tauri/test-record.jsonl`. Assertions will check event order, one result per call ID, non-empty tool observations, and final history consistency.

## Frontend Design

The frontend will move stream application into a pure reducer module:

- input: current assistant turn state plus one `StreamPayload`
- output: updated `ChatMessage`
- no React state, no Tauri dependency

`useChatSession.ts` will only manage session lifecycle, history loading, subscription, and dispatching events into the reducer.

Tool result matching will prefer `toolCallId`, then fall back to the last pending call with the same name only for old history. `final_answer` will not append duplicate content. It may fill content only if no answer tokens were received for that turn.

`historyMessages.ts` will use IDs from history to attach tool results instead of adjacency guessing whenever the backend provides them.

## Testing Strategy

Backend targeted tests:

- reasoning cumulative chunks emit only delta tokens
- duplicate provider tool IDs are preserved and not collapsed incorrectly
- stream and non-stream tool failures both produce non-empty observations
- tool call IDs are preserved in assistant messages and tool messages

Frontend tests:

- reducer routes tokens into thinking during thinking phase and content after `think_end`
- reducer pairs tool result by `toolCallId`
- final answer does not duplicate streamed content
- replaying a recorded event sequence yields one completed tool call and one final answer

Real trace test:

- run `cargo test --manifest-path apps\echo-tauri\src-tauri\Cargo.toml record_real_frontend_backend_stream_trace -- --ignored --nocapture`
- write `apps\echo-tauri\src-tauri\test-record.jsonl`
- assert `done.ok == true`, tool result content is non-empty, and tool call/result IDs pair.

Do not use `npm run build` or `cargo check` for this work unless the user explicitly changes that constraint.
