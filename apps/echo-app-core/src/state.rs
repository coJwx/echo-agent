//! 应用层共享状态。
//!
//! `AgentRegistry` 持有所有活跃会话的 `AgentHandle`，按 session_id 索引。
//! 流式执行直接走 echo_agent 内部的 `execution_mutex`（每个 ReactAgent 实例
//! 同一时刻只能跑一个 turn），所以这里只用 `RwLock<HashMap>` 保护索引本身，
//! 不需要给每个 handle 再加额外锁。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use echo_agent::prelude::{AgentHandle, ReactAgent, ReactAgentBuilder};
use echo_core::llm::types::Message;
use echo_state::memory::{
    ConversationStore, NewConversation, SqliteConversationStore, StoredMessage, project_messages,
};

use crate::error::{AppError, AppResult};

/// 会话元数据 —— 前端列表渲染用。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub model: String,
    pub system_prompt: String,
    pub work_dir: Option<String>,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
}

/// 创建会话时前端传入的参数。
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionInput {
    pub title: String,
    pub model: String,
    pub system_prompt: String,
    /// 可选：温度，None 时走模型默认
    pub temperature: Option<f32>,
    /// 可选：max_tokens
    pub max_tokens: Option<usize>,
    /// 可选：会话工作目录，None 时使用进程 CWD
    pub work_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateSessionModelInput {
    pub model: String,
}

/// JSON-backed storage for desktop session metadata.
pub struct SessionMetaStore {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl SessionMetaStore {
    pub fn new(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::PersistenceInit(e.to_string()))?;
        }
        Ok(Self {
            path,
            write_lock: Mutex::new(()),
        })
    }

    pub async fn load_all(&self) -> AppResult<Vec<SessionMeta>> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::PersistenceRead(e.to_string())),
        };
        let mut metas: Vec<SessionMeta> =
            serde_json::from_str(&raw).map_err(|e| AppError::PersistenceRead(e.to_string()))?;
        sort_session_metas(&mut metas);
        Ok(metas)
    }

    pub async fn upsert(&self, meta: SessionMeta) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut metas = self.load_all_unlocked().await?;
        if let Some(existing) = metas.iter_mut().find(|m| m.id == meta.id) {
            *existing = meta;
        } else {
            metas.push(meta);
        }
        sort_session_metas(&mut metas);
        self.flush_unlocked(&metas).await
    }

    pub async fn delete(&self, session_id: &str) -> AppResult<()> {
        let _guard = self.write_lock.lock().await;
        let mut metas = self.load_all_unlocked().await?;
        metas.retain(|m| m.id != session_id);
        self.flush_unlocked(&metas).await
    }

    async fn load_all_unlocked(&self) -> AppResult<Vec<SessionMeta>> {
        let raw = match tokio::fs::read_to_string(&self.path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::PersistenceRead(e.to_string())),
        };
        serde_json::from_str(&raw).map_err(|e| AppError::PersistenceRead(e.to_string()))
    }

    async fn flush_unlocked(&self, metas: &[SessionMeta]) -> AppResult<()> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        }

        let json = serde_json::to_string_pretty(metas)
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        let tmp_path = self.path.with_extension(format!(
            "{}.tmp",
            self.path
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("json")
        ));

        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        file.write_all(json.as_bytes())
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        file.sync_all()
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        drop(file);

        if tokio::fs::try_exists(&self.path)
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?
        {
            tokio::fs::remove_file(&self.path)
                .await
                .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        }
        if let Err(e) = tokio::fs::rename(&tmp_path, &self.path).await {
            let _ = tokio::fs::remove_file(&tmp_path).await;
            return Err(AppError::PersistenceWrite(e.to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AgentRegistryPaths {
    pub sessions_path: PathBuf,
    pub conversations_path: PathBuf,
}
impl AgentRegistryPaths {
    pub fn from_app_data_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            sessions_path: dir.join("sessions.json"),
            conversations_path: dir.join("conversations.sqlite"),
        }
    }

    #[cfg(test)]
    pub fn root_dir(&self) -> Option<&Path> {
        self.sessions_path.parent()
    }
}

struct SessionSlot {
    meta: SessionMeta,
    handle: AgentHandle,
}

/// 进程级 Agent 会话注册表。
pub struct AgentRegistry {
    sessions: RwLock<HashMap<String, SessionSlot>>,
    meta_store: SessionMetaStore,
    conversation_store: Arc<dyn ConversationStore>,
}

impl AgentRegistry {
    pub async fn new(paths: AgentRegistryPaths) -> AppResult<Self> {
        let meta_store = SessionMetaStore::new(paths.sessions_path)?;
        let conversation_store = Arc::new(
            SqliteConversationStore::new(paths.conversations_path)
                .map_err(|e| AppError::PersistenceInit(e.to_string()))?,
        );

        let metas = meta_store.load_all().await?;
        let mut sessions = HashMap::new();
        for meta in metas {
            let handle =
                build_session_handle(&meta, conversation_store.clone(), meta.work_dir.clone())?;
            sessions.insert(meta.id.clone(), SessionSlot { meta, handle });
        }

        Ok(Self {
            sessions: RwLock::new(sessions),
            meta_store,
            conversation_store,
        })
    }

    /// 构建一个新 ReactAgent 并以 AgentHandle 形式注册。
    pub async fn create(&self, input: CreateSessionInput) -> AppResult<SessionMeta> {
        let id = Uuid::new_v4().to_string();
        let now = now_ms();

        // 注意：当前 echo_agent v0.2 的 ReactAgentBuilder 没有 temperature/max_tokens
        // 直接 setter，需要走 .llm_config(LlmConfig) 路径。v0 MVP 先不暴露，
        // 待 v0.2 模块迭代时通过 LlmConfig 注入。
        let _ = (input.temperature, input.max_tokens);

        let meta = SessionMeta {
            id: id.clone(),
            title: input.title,
            model: input.model,
            system_prompt: input.system_prompt,
            work_dir: input.work_dir.clone(),
            created_at_ms: now,
            updated_at_ms: now,
        };

        let handle = build_session_handle(
            &meta,
            self.conversation_store.clone(),
            input.work_dir,
        )?;

        self.meta_store.upsert(meta.clone()).await?;

        self.sessions.write().await.insert(
            id.clone(),
            SessionSlot {
                meta: meta.clone(),
                handle,
            },
        );

        Ok(meta)
    }

    pub async fn update_model(&self, session_id: &str, model: String) -> AppResult<SessionMeta> {
        let model = model.trim().to_string();
        if model.is_empty() {
            return Err(AppError::Config("model 不能为空".to_string()));
        }

        let mut meta = {
            let guard = self.sessions.read().await;
            guard
                .get(session_id)
                .map(|slot| slot.meta.clone())
                .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?
        };
        meta.model = model;
        meta.updated_at_ms = now_ms();

        let handle = build_session_handle(
            &meta,
            self.conversation_store.clone(),
            meta.work_dir.clone(),
        )?;

        {
            let mut guard = self.sessions.write().await;
            guard.insert(
                session_id.to_string(),
                SessionSlot {
                    meta: meta.clone(),
                    handle,
                },
            );
        }

        self.meta_store.upsert(meta.clone()).await?;
        Ok(meta)
    }

    /// 拿到 session 对应的 AgentHandle（Arc 克隆，便于跨 await 持有）。
    pub async fn handle(&self, session_id: &str) -> AppResult<AgentHandle> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .map(|s| s.handle.clone())
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))
    }

    /// 列出所有会话（按 updated_at_ms 倒序）。
    pub async fn list(&self) -> Vec<SessionMeta> {
        let guard = self.sessions.read().await;
        let mut metas: Vec<SessionMeta> = guard.values().map(|s| s.meta.clone()).collect();
        sort_session_metas(&mut metas);
        metas
    }

    /// 删除一个会话。
    pub async fn delete(&self, session_id: &str) -> AppResult<()> {
        let mut guard = self.sessions.write().await;
        guard
            .remove(session_id)
            .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
        drop(guard);

        self.meta_store.delete(session_id).await?;
        self.conversation_store
            .delete_conversation(session_id)
            .await
            .map_err(|e| AppError::PersistenceDelete(e.to_string()))?;
        Ok(())
    }

    /// 更新 updated_at 戳，对话有新消息后调用。
    pub async fn touch(&self, session_id: &str) -> AppResult<()> {
        let meta = {
            let mut guard = self.sessions.write().await;
            let slot = guard
                .get_mut(session_id)
                .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
            slot.meta.updated_at_ms = now_ms();
            slot.meta.clone()
        };
        self.meta_store.upsert(meta).await
    }

    /// 读取该会话的对话历史（精简版）。
    ///
    /// 优先从 agent 内存读取；agent 内存为空（如重启后）时
    /// 回退到 `ConversationStore` 持久化记录。
    pub async fn history(&self, session_id: &str) -> AppResult<Vec<ChatTurn>> {
        let handle = self.handle(session_id).await?;
        let msgs: Vec<Message> = handle.read_async(|a| Box::pin(a.get_messages())).await;
        if msgs.len() <= 1 {
            let stored = self
                .conversation_store
                .get_messages(session_id)
                .await
                .map_err(|e| AppError::PersistenceRead(e.to_string()))?;
            if stored.len() > 1 {
                return Ok(stored_messages_to_chat_turns(&stored));
            }
        }
        Ok(messages_to_chat_turns(&msgs))
    }

    pub async fn persist_session_state(&self, session_id: &str) -> AppResult<()> {
        let (handle, title) = {
            let guard = self.sessions.read().await;
            let slot = guard
                .get(session_id)
                .ok_or_else(|| AppError::SessionNotFound(session_id.to_string()))?;
            (slot.handle.clone(), slot.meta.title.clone())
        };
        let messages: Vec<Message> = handle.read_async(|a| Box::pin(a.get_messages())).await;

        self.conversation_store
            .ensure_conversation(NewConversation {
                conversation_id: session_id.to_string(),
                user_id: "default".to_string(),
                agent_type: Some("react".to_string()),
                title: Some(title),
            })
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        let projected = project_messages(session_id, &messages)
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;
        self.conversation_store
            .save_messages(session_id, &projected)
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))
    }
}

/// Frontend history view: one visible chat turn with ordered render segments.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChatTurn {
    pub id: String,
    pub role: ChatTurnRole,
    pub segments: Vec<ChatSegment>,
    pub elapsed_ms: Option<u64>,
    pub status: ChatTurnStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChatTurnStatus {
    Done,
    Streaming,
    Error,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatSegment {
    Text { content: String },
    Thinking { content: String },
    ToolCall { call: ToolCallView },
}

impl ChatSegment {
    #[cfg(test)]
    fn kind(&self) -> &'static str {
        match self {
            ChatSegment::Text { .. } => "text",
            ChatSegment::Thinking { .. } => "thinking",
            ChatSegment::ToolCall { .. } => "tool_call",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ToolCallView {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
}

fn messages_to_chat_turns(messages: &[Message]) -> Vec<ChatTurn> {
    use echo_core::llm::types::Role;

    let mut turns = Vec::new();
    let mut pending_assistant: Option<ChatTurn> = None;
    let mut pushed_system = false;

    for (idx, message) in messages.iter().enumerate() {
        match &message.role {
            Role::System => {
                flush_assistant(&mut turns, &mut pending_assistant);
                if !pushed_system {
                    let before = turns.len();
                    push_text_turn(
                        &mut turns,
                        format!("system-{idx}"),
                        ChatTurnRole::System,
                        message.text_content(),
                    );
                    pushed_system = turns.len() > before;
                }
            }
            Role::User => {
                flush_assistant(&mut turns, &mut pending_assistant);
                push_text_turn(&mut turns, format!("user-{idx}"), ChatTurnRole::User, message.text_content());
            }
            Role::Assistant => {
                let turn = pending_assistant.get_or_insert_with(|| ChatTurn {
                    id: format!("assistant-{idx}"),
                    role: ChatTurnRole::Assistant,
                    segments: Vec::new(),
                    elapsed_ms: None,
                    status: ChatTurnStatus::Done,
                });
                append_assistant_message_segments(
                    turn,
                    message.reasoning_content.as_deref(),
                    message.tool_calls.as_deref(),
                    message.text_content().as_deref(),
                );
            }
            Role::Tool => {
                if let Some(turn) = pending_assistant.as_mut() {
                    patch_tool_result(
                        turn,
                        message.tool_call_id.as_deref(),
                        message.name.as_deref(),
                        message.text_content().unwrap_or_default(),
                    );
                }
            }
            Role::Custom(_) => {}
        }
    }

    flush_assistant(&mut turns, &mut pending_assistant);
    turns
}

fn stored_messages_to_chat_turns(messages: &[StoredMessage]) -> Vec<ChatTurn> {
    let mut turns = Vec::new();
    let mut pending_assistant: Option<ChatTurn> = None;
    let mut pushed_system = false;

    for (idx, message) in messages.iter().enumerate() {
        match message.role.as_str() {
            "system" => {
                flush_assistant(&mut turns, &mut pending_assistant);
                if !pushed_system {
                    let before = turns.len();
                    push_text_turn(
                        &mut turns,
                        format!("system-{idx}"),
                        ChatTurnRole::System,
                        message.content.clone(),
                    );
                    pushed_system = turns.len() > before;
                }
            }
            "user" => {
                flush_assistant(&mut turns, &mut pending_assistant);
                push_text_turn(&mut turns, format!("user-{idx}"), ChatTurnRole::User, message.content.clone());
            }
            "assistant" => {
                let turn = pending_assistant.get_or_insert_with(|| ChatTurn {
                    id: format!("assistant-{idx}"),
                    role: ChatTurnRole::Assistant,
                    segments: Vec::new(),
                    elapsed_ms: None,
                    status: ChatTurnStatus::Done,
                });
                let tool_calls = parse_stored_tool_calls(message.tool_calls_json.as_deref());
                append_assistant_message_segments(
                    turn,
                    message.reasoning_content.as_deref(),
                    Some(tool_calls.as_slice()),
                    message.content.as_deref(),
                );
            }
            "tool" => {
                if let Some(turn) = pending_assistant.as_mut() {
                    let (tool_call_id, name) = parse_tool_result_meta(message.tool_result_json.as_deref());
                    patch_tool_result(
                        turn,
                        tool_call_id.as_deref(),
                        name.as_deref(),
                        message.content.clone().unwrap_or_default(),
                    );
                }
            }
            _ => {}
        }
    }

    flush_assistant(&mut turns, &mut pending_assistant);
    turns
}

fn push_text_turn(
    turns: &mut Vec<ChatTurn>,
    id: String,
    role: ChatTurnRole,
    content: Option<String>,
) {
    let Some(content) = content.filter(|content| !content.trim().is_empty()) else {
        return;
    };
    turns.push(ChatTurn {
        id,
        role,
        segments: vec![ChatSegment::Text { content }],
        elapsed_ms: None,
        status: ChatTurnStatus::Done,
    });
}

fn flush_assistant(turns: &mut Vec<ChatTurn>, pending: &mut Option<ChatTurn>) {
    if let Some(turn) = pending.take()
        && !turn.segments.is_empty()
    {
        turns.push(turn);
    }
}

fn append_assistant_message_segments(
    turn: &mut ChatTurn,
    reasoning_content: Option<&str>,
    tool_calls: Option<&[echo_core::llm::types::ToolCall]>,
    content: Option<&str>,
) {
    if let Some(reasoning) = reasoning_content.filter(|reasoning| !reasoning.trim().is_empty()) {
        turn.segments.push(ChatSegment::Thinking {
            content: reasoning.to_string(),
        });
    }

    for call in tool_calls.unwrap_or_default() {
        turn.segments.push(ChatSegment::ToolCall {
            call: tool_call_view(call),
        });
    }

    if let Some(content) = content.filter(|content| !content.trim().is_empty()) {
        turn.segments.push(ChatSegment::Text {
            content: content.to_string(),
        });
    }
}

fn tool_call_view(call: &echo_core::llm::types::ToolCall) -> ToolCallView {
    ToolCallView {
        id: call.id.clone(),
        name: call.function.name.clone(),
        args: serde_json::from_str(&call.function.arguments)
            .unwrap_or_else(|_| serde_json::Value::String(call.function.arguments.clone())),
        result: None,
        error: None,
    }
}

fn patch_tool_result(
    turn: &mut ChatTurn,
    tool_call_id: Option<&str>,
    name: Option<&str>,
    result: String,
) {
    let Some(tool_call_id) = tool_call_id else {
        return;
    };
    for segment in &mut turn.segments {
        let ChatSegment::ToolCall { call } = segment else {
            continue;
        };
        if call.id == tool_call_id && name.is_none_or(|name| call.name == name) {
            call.result = Some(result);
            return;
        }
    }
}

fn parse_stored_tool_calls(json: Option<&str>) -> Vec<echo_core::llm::types::ToolCall> {
    json.and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

fn parse_tool_result_meta(json: Option<&str>) -> (Option<String>, Option<String>) {
    json.and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        .map(|v| {
            (
                v.get("tool_call_id").and_then(|v| v.as_str()).map(String::from),
                v.get("name").and_then(|v| v.as_str()).map(String::from),
            )
        })
        .unwrap_or((None, None))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn sort_session_metas(metas: &mut [SessionMeta]) {
    metas.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
}

fn build_session_handle(
    meta: &SessionMeta,
    conversation_store: Arc<dyn ConversationStore>,
    work_dir: Option<String>,
) -> AppResult<AgentHandle> {
    let work_dir_path = work_dir.map(PathBuf::from);

    let mut agent: ReactAgent = ReactAgentBuilder::new()
        .name(meta.title.clone())
        .model(meta.model.clone())
        .system_prompt(meta.system_prompt.clone())
        .max_iterations(30)
        .enable_tools()
        .working_dir(work_dir_path)
        .conversation_id(meta.id.clone())
        .build()
        .map_err(|e| AppError::AgentBuild(e.to_string()))?;

    agent.set_conversation_store(conversation_store);
    Ok(AgentHandle::new(agent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use echo_core::llm::types::{FunctionCall, MessageContent, ToolCall};
    use std::path::PathBuf;

    fn temp_store_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("echo-tauri-{name}-{}.json", Uuid::new_v4()))
    }

    fn temp_registry_paths(name: &str) -> AgentRegistryPaths {
        let dir = std::env::temp_dir().join(format!("echo-tauri-{name}-{}", Uuid::new_v4()));
        AgentRegistryPaths {
            sessions_path: dir.join("sessions.json"),
            conversations_path: dir.join("conversations.sqlite"),
        }
    }
    fn meta(id: &str, updated_at_ms: u128) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            title: format!("session-{id}"),
            model: "test-model".to_string(),
            system_prompt: "You are testing".to_string(),
            work_dir: None,
            created_at_ms: 10,
            updated_at_ms,
        }
    }

    #[tokio::test]
    async fn session_meta_store_loads_empty_when_missing() {
        let path = temp_store_path("missing");
        let store = SessionMetaStore::new(path.clone()).unwrap();

        let loaded = store.load_all().await.unwrap();

        assert!(loaded.is_empty());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn session_meta_store_upserts_and_sorts_by_updated_at_desc() {
        let path = temp_store_path("upsert-sort");
        let store = SessionMetaStore::new(path.clone()).unwrap();

        store.upsert(meta("old", 100)).await.unwrap();
        store.upsert(meta("new", 300)).await.unwrap();
        store.upsert(meta("middle", 200)).await.unwrap();

        let loaded = store.load_all().await.unwrap();

        assert_eq!(
            loaded.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["new", "middle", "old"]
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn session_meta_store_replaces_existing_session() {
        let path = temp_store_path("replace");
        let store = SessionMetaStore::new(path.clone()).unwrap();

        store.upsert(meta("same", 100)).await.unwrap();
        let mut updated = meta("same", 500);
        updated.title = "updated title".to_string();
        store.upsert(updated).await.unwrap();

        let loaded = store.load_all().await.unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "updated title");
        assert_eq!(loaded[0].updated_at_ms, 500);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn session_meta_store_deletes_session() {
        let path = temp_store_path("delete");
        let store = SessionMetaStore::new(path.clone()).unwrap();

        store.upsert(meta("keep", 100)).await.unwrap();
        store.upsert(meta("delete", 200)).await.unwrap();
        store.delete("delete").await.unwrap();

        let loaded = store.load_all().await.unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "keep");
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn session_meta_store_reports_malformed_json_as_read_error() {
        let path = temp_store_path("malformed");
        std::fs::write(&path, "{not valid json").unwrap();
        let store = SessionMetaStore::new(path.clone()).unwrap();

        let err = store.load_all().await.unwrap_err();

        assert!(matches!(err, AppError::PersistenceRead(_)));
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn agent_registry_rehydrates_sessions_from_metadata_store() {
        let paths = temp_registry_paths("rehydrate");
        let store = SessionMetaStore::new(paths.sessions_path.clone()).unwrap();
        store.upsert(meta("first", 100)).await.unwrap();
        store.upsert(meta("second", 300)).await.unwrap();

        let registry = AgentRegistry::new(paths.clone()).await.unwrap();

        let loaded = registry.list().await;
        assert_eq!(
            loaded.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            vec!["second", "first"]
        );
        assert_eq!(loaded[0].title, "session-second");
        assert_eq!(loaded[0].model, "test-model");
        assert_eq!(loaded[0].system_prompt, "You are testing");
        assert!(registry.handle("second").await.is_ok());

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    fn create_input(title: &str) -> CreateSessionInput {
        CreateSessionInput {
            title: title.to_string(),
            model: "test-model".to_string(),
            system_prompt: "You are testing".to_string(),
            temperature: None,
            max_tokens: None,
            work_dir: None,
        }
    }

    fn tool_call(id: &str, name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            call_type: "function".to_string(),
            function: FunctionCall {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    fn turn_text(turn: &ChatTurn) -> String {
        turn.segments
            .iter()
            .filter_map(|segment| match segment {
                ChatSegment::Text { content } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn chat_turns_preserve_assistant_segment_order_and_tool_results() {
        let mut first_assistant = Message::assistant_with_tools(vec![tool_call(
            "call_find",
            "find",
            r#"{"path":"."}"#,
        )]);
        first_assistant.reasoning_content = Some("Need to inspect the tree.".to_string());
        first_assistant.content = MessageContent::Text("I will inspect files.".to_string());

        let mut second_assistant = Message::assistant("This is a Rust project.".to_string());
        second_assistant.reasoning_content = Some("Now I can summarize.".to_string());

        let messages = vec![
            Message::user("Analyze this project".to_string()),
            first_assistant,
            Message::tool_result(
                "call_find".to_string(),
                "find".to_string(),
                "Cargo.toml\nsrc".to_string(),
            ),
            second_assistant,
        ];

        let turns = messages_to_chat_turns(&messages);

        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, ChatTurnRole::User);
        assert_eq!(turns[1].role, ChatTurnRole::Assistant);
        assert_eq!(
            turns[1]
                .segments
                .iter()
                .map(|segment| segment.kind())
                .collect::<Vec<_>>(),
            vec!["thinking", "tool_call", "text", "thinking", "text"]
        );

        match &turns[1].segments[1] {
            ChatSegment::ToolCall { call } => {
                assert_eq!(call.id, "call_find");
                assert_eq!(call.name, "find");
                assert_eq!(call.args, serde_json::json!({ "path": "." }));
                assert_eq!(call.result.as_deref(), Some("Cargo.toml\nsrc"));
                assert_eq!(call.error, None);
            }
            other => panic!("expected tool_call segment, got {other:?}"),
        }
    }

    #[test]
    fn chat_turns_keep_only_first_system_message_in_visible_history() {
        let messages = vec![
            Message::system("You are testing".to_string()),
            Message::user("hello".to_string()),
            Message::system("Repeated runtime prompt".to_string()),
            Message::assistant("hi".to_string()),
        ];

        let turns = messages_to_chat_turns(&messages);

        assert_eq!(
            turns.iter().map(|turn| &turn.role).collect::<Vec<_>>(),
            vec![
                &ChatTurnRole::System,
                &ChatTurnRole::User,
                &ChatTurnRole::Assistant
            ]
        );
        assert_eq!(turn_text(&turns[0]), "You are testing");
        assert_eq!(turn_text(&turns[1]), "hello");
        assert_eq!(turn_text(&turns[2]), "hi");
    }

    #[test]
    fn stored_messages_assemble_to_chat_turn_segments() {
        let stored = vec![
            StoredMessage {
                id: Some(1),
                conversation_id: "conversation-1".to_string(),
                role: "user".to_string(),
                content: Some("Analyze this project".to_string()),
                attachments_json: None,
                tool_calls_json: None,
                tool_result_json: None,
                reasoning_content: None,
                created_at: "2026-06-17T00:00:00Z".to_string(),
            },
            StoredMessage {
                id: Some(2),
                conversation_id: "conversation-1".to_string(),
                role: "assistant".to_string(),
                content: Some("I will inspect files.".to_string()),
                attachments_json: None,
                tool_calls_json: Some(
                    serde_json::json!([
                        {
                            "id": "call_find",
                            "type": "function",
                            "function": {
                                "name": "find",
                                "arguments": "{\"path\":\".\"}"
                            }
                        }
                    ])
                    .to_string(),
                ),
                tool_result_json: None,
                reasoning_content: Some("Need to inspect the tree.".to_string()),
                created_at: "2026-06-17T00:00:01Z".to_string(),
            },
            StoredMessage {
                id: Some(3),
                conversation_id: "conversation-1".to_string(),
                role: "tool".to_string(),
                content: Some("Cargo.toml\nsrc".to_string()),
                attachments_json: None,
                tool_calls_json: None,
                tool_result_json: Some(
                    serde_json::json!({
                        "tool_call_id": "call_find",
                        "name": "find"
                    })
                    .to_string(),
                ),
                reasoning_content: None,
                created_at: "2026-06-17T00:00:02Z".to_string(),
            },
        ];

        let turns = stored_messages_to_chat_turns(&stored);

        assert_eq!(turns.len(), 2);
        assert_eq!(
            turns[1]
                .segments
                .iter()
                .map(|segment| segment.kind())
                .collect::<Vec<_>>(),
            vec!["thinking", "tool_call", "text"]
        );
        match &turns[1].segments[1] {
            ChatSegment::ToolCall { call } => {
                assert_eq!(call.result.as_deref(), Some("Cargo.toml\nsrc"));
            }
            other => panic!("expected tool_call segment, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn agent_registry_create_persists_metadata_before_returning() {
        let paths = temp_registry_paths("create-persists");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();

        let created = registry.create(create_input("persist me")).await.unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        let sessions = reloaded.list().await;
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, created.id);
        assert_eq!(sessions[0].title, "persist me");

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_touch_persists_updated_timestamp() {
        let paths = temp_registry_paths("touch-persists");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let first = registry.create(create_input("first")).await.unwrap();
        let second = registry.create(create_input("second")).await.unwrap();

        registry.touch(&first.id).await.unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        let sessions = reloaded.list().await;
        assert_eq!(sessions[0].id, first.id);
        assert_eq!(sessions[1].id, second.id);

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_delete_removes_metadata_and_conversation() {
        let paths = temp_registry_paths("delete-cleanup");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let created = registry.create(create_input("delete me")).await.unwrap();
        // 写入一些对话数据
        let handle = registry.handle(&created.id).await.unwrap();
        handle
            .read_async(|agent| {
                Box::pin(agent.load_messages(vec![
                    Message::user("remember me".to_string()),
                    Message::assistant("ok".to_string()),
                ]))
            })
            .await;
        registry.persist_session_state(&created.id).await.unwrap();

        registry.delete(&created.id).await.unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        assert!(reloaded.list().await.is_empty());
        let stored = reloaded
            .conversation_store
            .get_messages(&created.id)
            .await
            .unwrap();
        assert!(stored.is_empty());

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_history_reads_from_conversation_store_after_restart() {
        let paths = temp_registry_paths("history-conv");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let created = registry.create(create_input("history")).await.unwrap();
        let handle = registry.handle(&created.id).await.unwrap();
        handle
            .read_async(|agent| {
                Box::pin(agent.load_messages(vec![
                    Message::user("hello".to_string()),
                    Message::assistant("hi there".to_string()),
                ]))
            })
            .await;
        registry.persist_session_state(&created.id).await.unwrap();

        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();
        let history = reloaded.history(&created.id).await.unwrap();

        assert_eq!(
            history
                .iter()
                .map(turn_text)
                .collect::<Vec<_>>(),
            vec!["hello", "hi there"]
        );

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_persists_runtime_state_to_transcript() {
        let paths = temp_registry_paths("runtime-persist");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let created = registry.create(create_input("runtime")).await.unwrap();
        let handle = registry.handle(&created.id).await.unwrap();
        handle
            .read_async(|agent| {
                Box::pin(agent.load_messages(vec![
                    Message::user("stream question".to_string()),
                    Message::assistant("stream answer".to_string()),
                ]))
            })
            .await;

        registry.persist_session_state(&created.id).await.unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        let transcript = reloaded
            .conversation_store
            .get_messages(&created.id)
            .await
            .unwrap();
        assert_eq!(
            transcript
                .iter()
                .map(|m| m.content.as_deref().unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["stream question", "stream answer"]
        );

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }
}
