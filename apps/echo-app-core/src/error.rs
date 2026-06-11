use serde::Serialize;

/// 应用层错误：跨 Tauri command 边界时统一转 String，
/// 前端只关心 message。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("session `{0}` 不存在")]
    SessionNotFound(String),

    #[error("agent 构建失败: {0}")]
    AgentBuild(String),

    #[error("agent 执行失败: {0}")]
    AgentExec(String),

    #[error("持久化初始化失败: {0}")]
    PersistenceInit(String),

    #[error("持久化读取失败: {0}")]
    PersistenceRead(String),

    #[error("持久化写入失败: {0}")]
    PersistenceWrite(String),

    #[error("持久化删除失败: {0}")]
    PersistenceDelete(String),

    // 以下两个 variant 暂未使用，但是设计期保留 —— Provider/MCP 模块上线后会用到
    #[allow(dead_code)]
    #[error("配置错误: {0}")]
    Config(String),

    #[allow(dead_code)]
    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<echo_agent::error::ReactError> for AppError {
    fn from(e: echo_agent::error::ReactError) -> Self {
        AppError::AgentExec(e.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
