//! Tauri commands —— 前端 invoke 的桥梁。

use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use echo_agent::prelude::Agent;

use crate::error::AppResult;
use crate::events::{StreamPayload, channel_name};
use crate::state::{AgentRegistry, CreateSessionInput, HistoryMessage, SessionMeta};

/// 创建会话。返回会话元数据，前端拿到 id 后立即可以发消息。
#[tauri::command]
pub async fn agent_create(
    registry: State<'_, Arc<AgentRegistry>>,
    input: CreateSessionInput,
) -> AppResult<SessionMeta> {
    registry.create(input).await
}

/// 列出所有会话（按更新时间倒序）。
#[tauri::command]
pub async fn agent_list_sessions(
    registry: State<'_, Arc<AgentRegistry>>,
) -> AppResult<Vec<SessionMeta>> {
    Ok(registry.list().await)
}

/// 删除会话。
#[tauri::command]
pub async fn agent_delete_session(
    registry: State<'_, Arc<AgentRegistry>>,
    session_id: String,
) -> AppResult<()> {
    registry.delete(&session_id).await
}

/// 读取会话历史消息（精简字段）。
///
/// 切换会话时前端调用，避免 UI 拿到空消息列表。
/// 走 `ReactAgent::get_messages` 异步路径
/// （`src/agent/react/mod.rs:709`）—— 同步版 `Agent::messages`
/// 会返回空（见 `echo-core/src/agent/mod.rs:556-558` 的注释）。
#[tauri::command]
pub async fn agent_history(
    registry: State<'_, Arc<AgentRegistry>>,
    session_id: String,
) -> AppResult<Vec<HistoryMessage>> {
    registry.history(&session_id).await
}

/// 调试用：同步收集一次真实模型流、事件序列和最终历史。
///
/// 这个命令会真正调用当前 session 配置的模型和工具，但不通过 Tauri event
/// channel 推送；它把同一条 stream 的 payload 按顺序返回，便于检查
/// ToolCall → ToolResult → history(tool message) 是否闭环。
#[derive(Debug, Serialize)]
pub struct DebugChatTrace {
    pub events: Vec<StreamPayload>,
    pub history: Vec<HistoryMessage>,
    pub elapsed_ms: u128,
    pub ok: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn agent_debug_chat_collect(
    registry: State<'_, Arc<AgentRegistry>>,
    session_id: String,
    message: String,
) -> AppResult<DebugChatTrace> {
    let start = Instant::now();
    let handle = registry.handle(&session_id).await?;
    registry.touch(&session_id).await?;
    let agent: Arc<dyn Agent> = handle.as_shared_agent().await;

    let mut events = Vec::new();
    let mut ok = true;
    let mut error = None;

    match agent.chat_stream(&message).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(event) => {
                        if let Some(payload) = StreamPayload::from_event(event) {
                            events.push(payload);
                        }
                    }
                    Err(e) => {
                        ok = false;
                        let message = e.to_string();
                        events.push(StreamPayload::Error {
                            source: "stream".to_string(),
                            message: message.clone(),
                        });
                        error = Some(message);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            ok = false;
            let message = e.to_string();
            events.push(StreamPayload::Error {
                source: "agent_debug_chat_collect".to_string(),
                message: message.clone(),
            });
            error = Some(message);
        }
    }

    if ok
        && let Err(e) = registry.persist_session_state(&session_id).await
    {
        ok = false;
        let message = e.to_string();
        events.push(StreamPayload::Error {
            source: "persistence".to_string(),
            message: message.clone(),
        });
        error = Some(message);
    }

    let history = registry.history(&session_id).await?;
    events.push(StreamPayload::Done {
        elapsed_ms: start.elapsed().as_millis(),
        ok,
        error: error.clone(),
    });

    Ok(DebugChatTrace {
        events,
        history,
        elapsed_ms: start.elapsed().as_millis(),
        ok,
        error,
    })
}

/// 发送消息并流式返回事件。
///
/// 这个 command 立即返回 `Ok(())` 表示"流已启动"；真正的 token / 工具事件
/// 通过 Tauri 事件通道 `echo://agent/stream/{session_id}` 推到前端。
///
/// 前端使用方式：
/// ```ts
/// import { invoke } from "@tauri-apps/api/core";
/// import { listen } from "@tauri-apps/api/event";
///
/// const unlisten = await listen(`echo://agent/stream/${sessionId}`, (e) => {
///     // e.payload: StreamPayload
/// });
/// await invoke("agent_chat_stream", { sessionId, message });
/// ```
#[tauri::command]
pub async fn agent_chat_stream(
    app: AppHandle,
    registry: State<'_, Arc<AgentRegistry>>,
    session_id: String,
    message: String,
) -> AppResult<()> {
    let handle = registry.handle(&session_id).await?;
    registry.touch(&session_id).await?;

    // 拿到 Arc<dyn Agent> 的"流安全"包装。其 chat_stream 实现内部 spawn
    // 一个独立 task 持有 RwLock 读锁，外部的流只通过 mpsc 消费 —— 这是
    // echo_agent 官方推荐的并发模式（src/agent/handle.rs:128-159）。
    let agent: Arc<dyn Agent> = handle.as_shared_agent().await;

    let app_clone = app.clone();
    let channel = channel_name(&session_id);
    let registry_clone = Arc::clone(registry.inner());
    let stream_session_id = session_id.clone();

    tokio::spawn(async move {
        let start = Instant::now();
        let mut ok = true;
        let mut final_err: Option<String> = None;

        match agent.chat_stream(&message).await {
            Ok(mut stream) => {
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(event) => {
                            if let Some(payload) = StreamPayload::from_event(event) {
                                if let Err(e) = app_clone.emit(&channel, &payload) {
                                    tracing::warn!(?e, "emit StreamPayload 失败");
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            ok = false;
                            let msg = e.to_string();
                            final_err = Some(msg.clone());
                            let _ = app_clone.emit(
                                &channel,
                                &StreamPayload::Error {
                                    source: "stream".to_string(),
                                    message: msg,
                                },
                            );
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                ok = false;
                let msg = e.to_string();
                final_err = Some(msg.clone());
                let _ = app_clone.emit(
                    &channel,
                    &StreamPayload::Error {
                        source: "agent_chat_stream".to_string(),
                        message: msg,
                    },
                );
            }
        }

        if ok
            && let Some(payload) = persistence_error_payload(
                registry_clone
                    .persist_session_state(&stream_session_id)
                    .await,
                &mut ok,
                &mut final_err,
            )
        {
            let _ = app_clone.emit(&channel, &payload);
        }

        // 收尾事件：无论成功失败都发，前端用它判断"输入框可以解锁"
        let _ = app_clone.emit(
            &channel,
            &StreamPayload::Done {
                elapsed_ms: start.elapsed().as_millis(),
                ok,
                error: final_err,
            },
        );
    });

    Ok(())
}

fn persistence_error_payload(
    result: AppResult<()>,
    ok: &mut bool,
    final_err: &mut Option<String>,
) -> Option<StreamPayload> {
    match result {
        Ok(()) => None,
        Err(e) => {
            *ok = false;
            let message = e.to_string();
            *final_err = Some(message.clone());
            Some(StreamPayload::Error {
                source: "persistence".to_string(),
                message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AgentRegistryPaths;

    #[test]
    fn persistence_failure_marks_stream_failed_and_returns_error_payload() {
        let mut ok = true;
        let mut final_err = None;

        let payload = persistence_error_payload(
            Err(crate::error::AppError::PersistenceWrite(
                "disk full".to_string(),
            )),
            &mut ok,
            &mut final_err,
        )
        .unwrap();

        assert!(!ok);
        assert_eq!(final_err.as_deref(), Some("持久化写入失败: disk full"));
        assert!(matches!(
            payload,
            StreamPayload::Error { source, message }
                if source == "persistence" && message == "持久化写入失败: disk full"
        ));
    }

    #[tokio::test]
    #[ignore = "calls the configured real model and writes src-tauri/test-record.jsonl"]
    async fn record_real_frontend_backend_stream_trace() {
        use std::io::Write;

        let record_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("test-record.jsonl");
        let mut record = std::fs::File::create(&record_path).unwrap();

        fn write_jsonl(
            record: &mut std::fs::File,
            phase: &str,
            value: serde_json::Value,
        ) {
            let line = serde_json::json!({
                "ts_ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis(),
                "phase": phase,
                "payload": value,
            });
            writeln!(record, "{}", serde_json::to_string(&line).unwrap()).unwrap();
            record.flush().unwrap();
        }

        let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        std::env::set_current_dir(&workspace_root).unwrap();
        unsafe {
            std::env::set_var(
                "ECHO_AGENT_MODELS_CONFIG",
                workspace_root.join("echo-agent-models.yaml"),
            );
        }
        let _ = dotenvy::from_path(workspace_root.join(".env"));

        write_jsonl(
            &mut record,
            "test.start",
            serde_json::json!({
                "workspace_root": workspace_root,
                "record_path": record_path,
            }),
        );

        let temp_dir = tempfile::tempdir().unwrap();
        let registry = Arc::new(
            AgentRegistry::new(AgentRegistryPaths::from_app_data_dir(temp_dir.path()))
                .await
                .unwrap(),
        );
        let input = CreateSessionInput {
            title: "stream-trace".to_string(),
            model: std::env::var("ECHO_AGENT_TEST_MODEL")
                .unwrap_or_else(|_| "glm-5.1".to_string()),
            system_prompt: "You are Echo. For the user's request, call list_dir exactly once with path \".\", then answer from the tool result. Do not call list_dir more than once.".to_string(),
            temperature: Some(0.0),
            max_tokens: Some(1024),
        };

        write_jsonl(
            &mut record,
            "frontend.invoke.agent_create",
            serde_json::json!({
                "title": input.title,
                "model": input.model,
                "system_prompt": input.system_prompt,
                "temperature": input.temperature,
                "max_tokens": input.max_tokens,
            }),
        );
        let session = registry.create(input).await.unwrap();
        write_jsonl(
            &mut record,
            "backend.response.agent_create",
            serde_json::to_value(&session).unwrap(),
        );

        let channel = channel_name(&session.id);
        write_jsonl(
            &mut record,
            "frontend.listen.subscribe",
            serde_json::json!({ "channel": channel }),
        );

        let message = "查看当前目录";
        write_jsonl(
            &mut record,
            "frontend.invoke.agent_chat_stream",
            serde_json::json!({
                "session_id": session.id,
                "message": message,
            }),
        );

        let start = Instant::now();
        let handle = registry.handle(&session.id).await.unwrap();
        registry.touch(&session.id).await.unwrap();
        let agent: Arc<dyn Agent> = handle.as_shared_agent().await;

        let mut ok = true;
        let mut final_error = None;
        let mut event_index = 0usize;
        let mut stream_tool_call_ids: Vec<String> = Vec::new();
        let mut stream_tool_result_ids: Vec<String> = Vec::new();
        let mut stream_tool_result_outputs: Vec<String> = Vec::new();
        match agent.chat_stream(message).await {
            Ok(mut stream) => {
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(event) => {
                            let event_debug = format!("{:?}", event);
                            if let Some(payload) = StreamPayload::from_event(event) {
                                match &payload {
                                    StreamPayload::ToolCall { tool_call_id, .. } => {
                                        stream_tool_call_ids.push(tool_call_id.clone());
                                    }
                                    StreamPayload::ToolResult {
                                        tool_call_id,
                                        output,
                                        ..
                                    } => {
                                        stream_tool_result_ids.push(tool_call_id.clone());
                                        stream_tool_result_outputs.push(output.clone());
                                    }
                                    _ => {}
                                }
                                write_jsonl(
                                    &mut record,
                                    "backend.emit.raw",
                                    serde_json::json!({
                                        "index": event_index,
                                        "channel": channel,
                                        "agent_event_debug": event_debug,
                                        "stream_payload": payload,
                                    }),
                                );
                                write_jsonl(
                                    &mut record,
                                    "frontend.listen.received.raw",
                                    serde_json::json!({
                                        "index": event_index,
                                        "channel": channel,
                                        "stream_payload": payload,
                                    }),
                                );
                                event_index += 1;
                            }
                        }
                        Err(e) => {
                            ok = false;
                            let message = e.to_string();
                            final_error = Some(message.clone());
                            write_jsonl(
                                &mut record,
                                "backend.stream.error",
                                serde_json::json!({ "error": message }),
                            );
                            break;
                        }
                    }
                }
            }
            Err(e) => {
                ok = false;
                let message = e.to_string();
                final_error = Some(message.clone());
                write_jsonl(
                    &mut record,
                    "backend.command.error",
                    serde_json::json!({ "error": message }),
                );
            }
        }

        if ok
            && let Err(e) = registry.persist_session_state(&session.id).await
        {
            ok = false;
            let message = e.to_string();
            final_error = Some(message.clone());
            write_jsonl(
                &mut record,
                "backend.persistence.error",
                serde_json::json!({ "error": message }),
            );
        }

        let done = StreamPayload::Done {
            elapsed_ms: start.elapsed().as_millis(),
            ok,
            error: final_error,
        };
        write_jsonl(
            &mut record,
            "backend.emit.raw",
            serde_json::json!({
                "index": event_index,
                "channel": channel,
                "stream_payload": done,
            }),
        );
        write_jsonl(
            &mut record,
            "frontend.listen.received.raw",
            serde_json::json!({
                "index": event_index,
                "channel": channel,
                "stream_payload": done,
            }),
        );

        let history = registry.history(&session.id).await.unwrap();
        write_jsonl(
            &mut record,
            "backend.history.after_stream",
            serde_json::json!({
                "session_id": session.id,
                "history": history,
            }),
        );

        let tool_call_count = history
            .iter()
            .filter(|m| {
                m.tool_calls
                    .as_ref()
                    .is_some_and(|calls| calls.iter().any(|call| call.name == "list_dir"))
            })
            .count();
        let tool_result_count = history
            .iter()
            .filter(|m| m.role == "tool" && !m.content.is_empty())
            .count();

        write_jsonl(
            &mut record,
            "test.assertions",
            serde_json::json!({
                "tool_call_messages_with_list_dir": tool_call_count,
                "non_empty_tool_result_messages": tool_result_count,
                "stream_tool_call_ids": stream_tool_call_ids.clone(),
                "stream_tool_result_ids": stream_tool_result_ids.clone(),
            }),
        );

        assert_eq!(tool_call_count, 1, "expected exactly one list_dir tool call message");
        assert!(
            tool_result_count >= 1,
            "expected at least one non-empty tool result message"
        );
        assert_eq!(
            stream_tool_call_ids.len(),
            1,
            "expected exactly one stream tool call id"
        );
        assert_eq!(
            stream_tool_result_ids, stream_tool_call_ids,
            "stream tool result ids must match tool call ids"
        );
        assert!(
            stream_tool_result_outputs
                .iter()
                .all(|output| !output.is_empty()),
            "stream tool result outputs must be non-empty"
        );
    }
}
