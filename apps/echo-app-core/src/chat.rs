use std::future::Future;
use std::sync::Arc;
use std::time::Instant;

use futures::StreamExt;
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::events::StreamPayload;
use crate::state::{AgentRegistry, HistoryMessage};

#[derive(Debug, Serialize)]
pub struct DebugChatTrace {
    pub events: Vec<StreamPayload>,
    pub history: Vec<HistoryMessage>,
    pub elapsed_ms: u128,
    pub ok: bool,
    pub error: Option<String>,
}

pub async fn collect_debug_chat(
    registry: Arc<AgentRegistry>,
    session_id: String,
    message: String,
) -> AppResult<DebugChatTrace> {
    let start = Instant::now();
    let mut events = Vec::new();
    let mut ok = true;
    let mut error = None;

    let stream_result = stream_chat_payloads(
        registry.clone(),
        &session_id,
        &message,
        "agent_debug_chat_collect",
        |payload| {
            events.push(payload);
            async { Ok(()) }
        },
    )
    .await;

    if let Err(e) = stream_result {
        ok = false;
        error = Some(e.to_string());
    }

    if ok
        && let Some(payload) = persistence_error_payload(
            registry.persist_session_state(&session_id).await,
            &mut ok,
            &mut error,
        )
    {
        events.push(payload);
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

pub async fn run_chat_stream<F, Fut>(
    registry: Arc<AgentRegistry>,
    session_id: String,
    message: String,
    mut emit: F,
) where
    F: FnMut(StreamPayload) -> Fut,
    Fut: Future<Output = AppResult<()>>,
{
    let start = Instant::now();
    let mut ok = true;
    let mut final_err = None;

    if let Err(e) = stream_chat_payloads(
        registry.clone(),
        &session_id,
        &message,
        "agent_chat_stream",
        |payload| emit(payload),
    )
    .await
    {
        ok = false;
        let message = e.to_string();
        final_err = Some(message.clone());
        let _ = emit(StreamPayload::Error {
            source: "agent_chat_stream".to_string(),
            message,
        })
        .await;
    }

    if ok
        && let Some(payload) = persistence_error_payload(
            registry.persist_session_state(&session_id).await,
            &mut ok,
            &mut final_err,
        )
    {
        let _ = emit(payload).await;
    }

    let _ = emit(StreamPayload::Done {
        elapsed_ms: start.elapsed().as_millis(),
        ok,
        error: final_err,
    })
    .await;
}

async fn stream_chat_payloads<F, Fut>(
    registry: Arc<AgentRegistry>,
    session_id: &str,
    message: &str,
    command_source: &str,
    mut emit: F,
) -> AppResult<()>
where
    F: FnMut(StreamPayload) -> Fut,
    Fut: Future<Output = AppResult<()>>,
{
    let handle = registry.handle(session_id).await?;
    registry.touch(session_id).await?;
    let agent = handle.as_shared_agent().await;

    match agent.chat_stream(message).await {
        Ok(mut stream) => {
            while let Some(item) = stream.next().await {
                match item {
                    Ok(event) => {
                        if let Some(payload) = StreamPayload::from_event(event)
                            && emit(payload).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(e) => {
                        let message = e.to_string();
                        let _ = emit(StreamPayload::Error {
                            source: "stream".to_string(),
                            message: message.clone(),
                        })
                        .await;
                        return Err(AppError::AgentExec(message));
                    }
                }
            }
            Ok(())
        }
        Err(e) => {
            let message = e.to_string();
            let _ = emit(StreamPayload::Error {
                source: command_source.to_string(),
                message: message.clone(),
            })
            .await;
            Err(AppError::AgentExec(message))
        }
    }
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
    use crate::events::channel_name;
    use crate::state::{AgentRegistryPaths, CreateSessionInput};

    #[test]
    fn persistence_failure_marks_stream_failed_and_returns_error_payload() {
        let mut ok = true;
        let mut final_err = None;

        let payload = persistence_error_payload(
            Err(AppError::PersistenceWrite("disk full".to_string())),
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
    #[ignore = "calls the configured real model and writes apps/echo-app-core/test-record.jsonl"]
    async fn record_real_chat_stream_trace() {
        use std::io::Write;

        let record_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-record.jsonl");
        let mut record = std::fs::File::create(&record_path).unwrap();

        fn write_jsonl(record: &mut std::fs::File, phase: &str, value: serde_json::Value) {
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
            .to_path_buf();
        std::env::set_current_dir(&workspace_root).unwrap();
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

        let mut event_index = 0usize;
        let mut stream_tool_call_ids: Vec<String> = Vec::new();
        let mut stream_tool_result_ids: Vec<String> = Vec::new();
        let mut stream_tool_result_outputs: Vec<String> = Vec::new();
        run_chat_stream(
            registry.clone(),
            session.id.clone(),
            message.to_string(),
            |payload| {
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
                async { Ok(()) }
            },
        )
        .await;

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

        assert_eq!(
            tool_call_count, 1,
            "expected exactly one list_dir tool call message"
        );
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
