//! 前端事件载荷定义。
//!
//! 与 AgentEvent 解耦：前端只关心一组扁平、可 JSON 化的形状，不需要看到
//! `BoxStream<Result<AgentEvent>>` 的复杂泛型。

use serde::Serialize;
use serde_json::Value;

use echo_agent::prelude::AgentEvent;

/// 发往前端的对话流事件
///
/// 与 `echo_agent::AgentEvent` 一一对应，但是结构更适合 serde_json 序列化
/// + Tauri event 通道传输：所有变体都带 `kind` 标签，前端可以 switch 渲染。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamPayload {
    /// LLM 流式 token
    Token { delta: String },
    /// 思考阶段开始
    ThinkStart,
    /// 思考阶段结束（含 token 计数）
    ThinkEnd {
        prompt_tokens: usize,
        completion_tokens: usize,
    },
    /// 工具调用开始
    ToolCall {
        tool_call_id: String,
        name: String,
        args: Value,
    },
    /// 工具结果
    ToolResult {
        tool_call_id: String,
        name: String,
        output: String,
    },
    /// 工具错误
    ToolError {
        tool_call_id: String,
        name: String,
        error: String,
    },
    /// 工具子事件（流式工具进度）—— 直接转 JSON
    ToolStream {
        tool_call_id: String,
        name: String,
        payload: Value,
    },
    /// Guard 触发
    GuardTriggered { guard: String, blocked: bool },
    /// 长期记忆召回
    MemoryRecalled { count: usize },
    /// 上下文自动压缩
    ContextCompressed {
        before_count: usize,
        after_count: usize,
        before_tokens: usize,
        after_tokens: usize,
    },
    /// vega-lite 图表 spec
    Chart { spec: Value },
    /// 通用错误（非工具错误）
    Error { source: String, message: String },
    /// 安全提示
    SafetyNotice {
        action: String,
        reason: String,
        risk: String,
        permission: String,
    },
    /// 工具参数校验失败
    ParameterError {
        tool: String,
        parameter: String,
        expected: String,
        got: String,
    },
    /// 最终回答
    FinalAnswer { text: String },
    /// 被取消
    Cancelled,
    /// 流结束（无论成功/失败）—— 前端关闭 spinner、解锁输入框
    Done {
        elapsed_ms: u128,
        ok: bool,
        error: Option<String>,
    },
}

impl StreamPayload {
    /// 把 echo_agent 内部 AgentEvent 转换为前端载荷。
    ///
    /// 注意 `#[non_exhaustive]` — 未来新增 variant 会落到 `_` 分支并丢弃，
    /// 显式而不是静默 panic。
    pub fn from_event(ev: AgentEvent) -> Option<Self> {
        let p = match ev {
            AgentEvent::Token(s) => Self::Token { delta: s },
            AgentEvent::ThinkStart => Self::ThinkStart,
            AgentEvent::ThinkEnd {
                prompt_tokens,
                completion_tokens,
            } => Self::ThinkEnd {
                prompt_tokens,
                completion_tokens,
            },
            AgentEvent::ToolCall {
                tool_call_id,
                name,
                args,
            } => Self::ToolCall {
                tool_call_id,
                name,
                args,
            },
            AgentEvent::ToolResult {
                tool_call_id,
                name,
                output,
            } => Self::ToolResult {
                tool_call_id,
                name,
                output,
            },
            AgentEvent::ToolError {
                tool_call_id,
                name,
                error,
            } => Self::ToolError {
                tool_call_id,
                name,
                error,
            },
            AgentEvent::ToolStream {
                tool_call_id,
                name,
                event,
            } => Self::ToolStream {
                tool_call_id,
                name,
                // ToolStreamEvent 用 Debug 序列化，避免引入它的 Serialize 依赖。
                // v0 够用，正式版替换成 serde_json::to_value(&event).
                payload: serde_json::json!({ "debug": format!("{:?}", event) }),
            },
            AgentEvent::GuardTriggered { guard, blocked } => {
                Self::GuardTriggered { guard, blocked }
            }
            AgentEvent::MemoryRecalled { count } => Self::MemoryRecalled { count },
            AgentEvent::ContextCompressed {
                before_count,
                after_count,
                before_tokens,
                after_tokens,
            } => Self::ContextCompressed {
                before_count,
                after_count,
                before_tokens,
                after_tokens,
            },
            AgentEvent::Chart { spec } => Self::Chart { spec },
            AgentEvent::Error { source, message } => Self::Error { source, message },
            AgentEvent::SafetyNotice {
                action,
                reason,
                risk,
                permission,
            } => Self::SafetyNotice {
                action,
                reason,
                risk,
                permission,
            },
            AgentEvent::ParameterError {
                tool,
                parameter,
                expected,
                got,
            } => Self::ParameterError {
                tool,
                parameter,
                expected,
                got,
            },
            AgentEvent::FinalAnswer(text) => Self::FinalAnswer { text },
            AgentEvent::Cancelled => Self::Cancelled,
            // 未来 variant：忽略，记录一行日志便于运维察觉
            other => {
                tracing::debug!(?other, "echo_tauri: 收到未识别的 AgentEvent，已丢弃");
                return None;
            }
        };
        Some(p)
    }
}

/// 拼接当前 session 的事件 channel 名 —— 前端 listen 时用
pub fn channel_name(session_id: &str) -> String {
    format!("echo://agent/stream/{}", session_id)
}
