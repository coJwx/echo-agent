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
use echo_core::tools::{Tool, ToolRegistrar};
use echo_state::memory::{
    Checkpointer, ConversationStore, FileCheckpointer, NewConversation, SqliteConversationStore,
    ThreadState, project_messages,
};
use echo_tools::register_all_tools;

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
    pub checkpoints_path: PathBuf,
    pub conversations_path: PathBuf,
}

impl AgentRegistryPaths {
    pub fn from_app_data_dir(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        Self {
            sessions_path: dir.join("sessions.json"),
            checkpoints_path: dir.join("checkpoints.json"),
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
    checkpointer: Arc<dyn Checkpointer>,
    conversation_store: Arc<dyn ConversationStore>,
}

impl AgentRegistry {
    pub async fn new(paths: AgentRegistryPaths) -> AppResult<Self> {
        let meta_store = SessionMetaStore::new(paths.sessions_path)?;
        let checkpointer = Arc::new(
            FileCheckpointer::new(paths.checkpoints_path)
                .map_err(|e| AppError::PersistenceInit(e.to_string()))?,
        );
        let conversation_store = Arc::new(
            SqliteConversationStore::new(paths.conversations_path)
                .map_err(|e| AppError::PersistenceInit(e.to_string()))?,
        );

        let metas = meta_store.load_all().await?;
        let mut sessions = HashMap::new();
        for meta in metas {
            let handle =
                build_session_handle(&meta, checkpointer.clone(), conversation_store.clone(), meta.work_dir.clone())?;
            sessions.insert(meta.id.clone(), SessionSlot { meta, handle });
        }

        Ok(Self {
            sessions: RwLock::new(sessions),
            meta_store,
            checkpointer,
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
            self.checkpointer.clone(),
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
            self.checkpointer.clone(),
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
        self.checkpointer
            .delete_session(session_id)
            .await
            .map_err(|e| AppError::PersistenceDelete(e.to_string()))?;
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
    /// 走 `ReactAgent::get_messages`（`src/agent/react/mod.rs:709`）的
    /// 异步路径——`Agent::messages()` 同步版注释明确说会返回空，必须用
    /// 异步路径。`AgentHandle::read_async` 提供正确的
    /// `tokio::sync::RwLock::read().await` 时机。
    pub async fn history(&self, session_id: &str) -> AppResult<Vec<HistoryMessage>> {
        let handle = self.handle(session_id).await?;
        let msgs: Vec<Message> = handle.read_async(|a| Box::pin(a.get_messages())).await;
        if msgs.len() <= 1
            && let Some(state) = self
                .checkpointer
                .get_state(session_id)
                .await
                .map_err(|e| AppError::PersistenceRead(e.to_string()))?
        {
            return Ok(state
                .messages
                .iter()
                .enumerate()
                .map(|(i, m)| history_message(m, i))
                .collect());
        }
        Ok(msgs
            .iter()
            .enumerate()
            .map(|(i, m)| history_message(m, i))
            .collect())
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

        self.checkpointer
            .put_state(session_id, ThreadState::from_messages(messages.clone()))
            .await
            .map_err(|e| AppError::PersistenceWrite(e.to_string()))?;

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

/// 前端历史列表用的精简消息形状。
/// 与 `apps/echo-tauri/src/types/index.ts::HistoryMessage` 字段一一对应。
#[derive(Debug, Clone, Serialize)]
pub struct HistoryMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub name: Option<String>,
    pub thinking_content: Option<String>,
    pub tool_calls: Option<Vec<HistoryToolCall>>,
    pub elapsed_ms: Option<u64>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryToolCall {
    pub id: String,
    pub name: String,
    pub args: serde_json::Value,
    pub result: Option<String>,
    pub error: Option<String>,
}

/// `Message` → `HistoryMessage` 的纯转换。
/// role 是字符串，避免前端的 Role 枚举需要再加变体。
fn history_message(m: &Message, idx: usize) -> HistoryMessage {
    use echo_core::llm::types::{MessageContent, Role};

    let role = match &m.role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
        Role::Custom(_) => "custom",
    };
    let id = format!("{role}-{idx}");

    let content = match &m.content {
        MessageContent::Text(s) => s.clone(),
        MessageContent::Parts(parts) => {
            // 多模态：把所有 Text 段拼起来。图片/音频等 non-text 段用
            // Debug 简写占位（v0 UI 不渲染图片，只占位）
            parts
                .iter()
                .map(|p| match p {
                    echo_core::llm::types::ContentPart::Text { text } => text.clone(),
                    other => format!("<{other:?}>"),
                })
                .collect::<Vec<_>>()
                .join("")
        }
        MessageContent::Empty => String::new(),
    };

    let tool_calls = m.tool_calls.as_ref().map(|calls| {
        calls
            .iter()
            .map(|c| HistoryToolCall {
                id: c.id.clone(),
                name: c.function.name.clone(),
                args: serde_json::from_str(&c.function.arguments)
                    .unwrap_or(serde_json::Value::String(c.function.arguments.clone())),
                result: None, // tool 结果在独立的 role=Tool 消息里，UI 会按 tool_call_id 配对
                error: None,
            })
            .collect::<Vec<_>>()
    });

    // 对历史消息，状态都标 done：
    //   - assistant:        done
    //   - user / system:    done
    //   - tool:             done
    let status = "done".to_string();

    HistoryMessage {
        id,
        role: role.to_string(),
        content,
        tool_call_id: m.tool_call_id.clone(),
        name: m.name.clone(),
        thinking_content: m.reasoning_content.clone(),
        tool_calls,
        elapsed_ms: None,
        status,
    }
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
    checkpointer: Arc<dyn Checkpointer>,
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
        .checkpointer(checkpointer, meta.id.clone())
        .conversation_id(meta.id.clone())
        .build()
        .map_err(|e| AppError::AgentBuild(e.to_string()))?;

    agent.set_conversation_store(conversation_store);
    Ok(AgentHandle::new(agent))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_store_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("echo-tauri-{name}-{}.json", Uuid::new_v4()))
    }

    fn temp_registry_paths(name: &str) -> AgentRegistryPaths {
        let dir = std::env::temp_dir().join(format!("echo-tauri-{name}-{}", Uuid::new_v4()));
        AgentRegistryPaths {
            sessions_path: dir.join("sessions.json"),
            checkpoints_path: dir.join("checkpoints.json"),
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
    async fn agent_registry_delete_removes_metadata_and_checkpoint() {
        let paths = temp_registry_paths("delete-cleanup");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let created = registry.create(create_input("delete me")).await.unwrap();
        registry
            .checkpointer
            .put_state(
                &created.id,
                echo_state::memory::ThreadState::from_messages(vec![Message::user(
                    "remember me".to_string(),
                )]),
            )
            .await
            .unwrap();

        registry.delete(&created.id).await.unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        assert!(reloaded.list().await.is_empty());
        assert!(
            reloaded
                .checkpointer
                .get_state(&created.id)
                .await
                .unwrap()
                .is_none()
        );

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_history_reads_checkpoint_after_restart() {
        let paths = temp_registry_paths("history-checkpoint");
        let registry = AgentRegistry::new(paths.clone()).await.unwrap();
        let created = registry.create(create_input("history")).await.unwrap();
        registry
            .checkpointer
            .put_state(
                &created.id,
                echo_state::memory::ThreadState::from_messages(vec![
                    Message::user("hello".to_string()),
                    Message::assistant("hi there".to_string()),
                ]),
            )
            .await
            .unwrap();
        let reloaded = AgentRegistry::new(paths.clone()).await.unwrap();

        let history = reloaded.history(&created.id).await.unwrap();

        assert_eq!(
            history
                .iter()
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>(),
            vec!["hello", "hi there"]
        );

        let _ = std::fs::remove_dir_all(paths.root_dir().unwrap());
    }

    #[tokio::test]
    async fn agent_registry_persists_runtime_state_to_checkpoint_and_transcript() {
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

        let checkpoint = reloaded
            .checkpointer
            .get_state(&created.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(checkpoint.messages.len(), 2);

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
