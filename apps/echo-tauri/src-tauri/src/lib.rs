//! Echo Agent Tauri 桌面客户端 —— 主入口
//!
//! 应用结构：
//! - `state`    : 进程级 AgentRegistry（按 session_id 索引 AgentHandle）
//! - `events`   : 前端事件载荷定义（StreamPayload）
//! - `commands` : 暴露给前端的 invoke 入口
//! - `error`    : 应用层错误类型

mod commands;
mod provider;

use std::sync::Arc;

use echo_app_core::state::{AgentRegistry, AgentRegistryPaths};
use tauri::Manager;

/// Tauri 入口点。`mobile` build 期望这个签名。
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 把工作目录定位到包含 .env 的目录
    // —— Tauri dev 时进程 cwd 是 `src-tauri/`，需要回到 `../../..`
    // 才能找到 `.env`
    bootstrap_workspace_root();

    // 加载 .env（echo_agent::config 内部也会调，这里多调一次保证
    // Tauri 启动早期就有 ENV）
    let _ = dotenvy::dotenv();

    // 初始化 tracing：默认 INFO，可通过 RUST_LOG 调
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,echo_tauri_lib=debug")
            }),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let paths = AgentRegistryPaths::from_app_data_dir(app_data_dir);
            let registry = tauri::async_runtime::block_on(AgentRegistry::new(paths))
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            app.manage(Arc::new(registry));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::agent_create,
            commands::agent_list_sessions,
            commands::agent_delete_session,
            commands::agent_update_model,
            commands::agent_history,
            commands::agent_debug_chat_collect,
            commands::agent_chat_stream,
            provider::provider_config_get,
            provider::provider_model_save,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 从当前 cwd 向上找到包含 `.env` 的目录并切过去。
///
/// 这样无论是：
/// - `cargo tauri dev`（cwd = `apps/echo-tauri/src-tauri/`）
/// - `cargo run`        （cwd 同上）
/// - 双击 `.exe`        （cwd 可能是任意位置）
/// 都能正确加载 `.env`。
///
/// 找不到则保持原 cwd。
fn bootstrap_workspace_root() {
    let Ok(mut cur) = std::env::current_dir() else {
        return;
    };
    for _ in 0..8 {
        let candidate = cur.join(".env");
        if candidate.is_file() {
            let _ = std::env::set_current_dir(&cur);
            return;
        }
        if !cur.pop() {
            break;
        }
    }
}
