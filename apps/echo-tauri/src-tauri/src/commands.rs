//! Tauri commands —— 前端 invoke 的桥梁。

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use echo_app_core::chat::{self, DebugChatTrace};
use echo_app_core::error::AppResult;
use echo_app_core::events::channel_name;
use echo_app_core::state::{AgentRegistry, CreateSessionInput, HistoryMessage, SessionMeta};

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
#[tauri::command]
pub async fn agent_debug_chat_collect(
    registry: State<'_, Arc<AgentRegistry>>,
    session_id: String,
    message: String,
) -> AppResult<DebugChatTrace> {
    chat::collect_debug_chat(Arc::clone(registry.inner()), session_id, message).await
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
    let app_clone = app.clone();
    let channel = channel_name(&session_id);
    let registry_clone = Arc::clone(registry.inner());

    tokio::spawn(async move {
        chat::run_chat_stream(registry_clone, session_id, message, move |payload| {
            let app = app_clone.clone();
            let channel = channel.clone();
            async move {
                app.emit(&channel, &payload)
                    .map_err(|e| echo_app_core::error::AppError::Internal(e.to_string()))
            }
        })
        .await;
    });

    Ok(())
}
