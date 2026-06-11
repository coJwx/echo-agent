# Echo Apps 架构与开发规则

`apps` 目录承载 Echo 的应用层。当前保留两种运行入口：

- `echo-tauri`：桌面应用入口，前端运行在 Tauri WebView 中，前后端通过 Tauri IPC 和 Tauri event 通道通信。
- `echo-http-server`：网页/移动端入口，启动本地 HTTP/WebSocket 服务，浏览器或手机通过同一个端口访问页面和接口。

两种入口共享同一套业务核心：

- `echo-app-core`：应用核心层，负责会话、模型配置、事件 payload、聊天流、持久化，以及所有对接 `echo-agent` / `echo-agents` / tools 的应用逻辑。

## 分层职责

### `apps/echo-app-core`

这里是唯一应该承载业务实现的地方。

应该放在这里的代码：

- 新功能的核心函数。
- 调用 `echo-agent`、`echo-agents`、`echo-core`、`echo-tools`、`echo-state` 等底层 crate 的代码。
- session 创建、删除、列表、历史读取等应用状态逻辑。
- provider/model 配置读取和保存逻辑。
- chat stream 执行、事件转换、错误处理、持久化、`Done` / `Error` payload 生成逻辑。
- 被 Tauri IPC 和 HTTP/WebSocket 共同复用的输入输出类型。
- 与协议无关的测试。

不应该放在这里的代码：

- Tauri `#[tauri::command]`。
- `AppHandle.emit`。
- Axum route、extractor、middleware。
- HTTP cookie、Bearer token、CORS、WebSocket frame 处理。
- 前端 transport 判断。

### `apps/echo-tauri`

这里是 Tauri 桌面入口，只做 IPC 适配。

Tauri 侧允许做的事情：

- 定义 `#[tauri::command]`。
- 从 `State<'_, Arc<AgentRegistry>>` 取共享状态。
- 把前端传入参数转给 `echo-app-core`。
- 把 `echo-app-core` 返回值直接返回给前端。
- 对 stream payload 调用 `app.emit(channel, payload)`。

Tauri 侧不应该重新实现：

- agent 调用逻辑。
- chat stream 循环。
- stream payload 转换。
- session/provider 的业务规则。
- 持久化、错误归一化、done/error 语义。

示例形态：

```rust
#[tauri::command]
pub async fn agent_create(
    registry: State<'_, Arc<AgentRegistry>>,
    input: CreateSessionInput,
) -> AppResult<SessionMeta> {
    registry.create(input).await
}
```

流式接口也应该只是把 core 产出的 payload 转成 Tauri event：

```rust
chat::run_chat_stream(registry, session_id, message, move |payload| {
    let app = app.clone();
    let channel = channel.clone();
    async move {
        app.emit(&channel, &payload)
            .map_err(|e| AppError::Internal(e.to_string()))
    }
})
.await;
```

### `apps/echo-http-server`

这里是网页/移动端入口，只做 HTTP/WebSocket 适配。

HTTP server 侧允许做的事情：

- 定义 Axum route。
- 处理 token 校验、cookie、Bearer、query token。
- 处理 CORS。
- 提供静态前端文件和 SPA fallback。
- 把 REST JSON body 转成 core 输入。
- 把 core 输出包装成 JSON response。
- 把 WebSocket 文本消息解析成命令，再把 core 产出的 payload 序列化发回 WebSocket。

HTTP server 侧不应该重新实现：

- agent 调用逻辑。
- chat stream 循环。
- stream payload 转换。
- session/provider 的业务规则。
- 持久化、错误归一化、done/error 语义。

示例形态：

```rust
async fn create_session(
    State(state): State<AppState>,
    Json(input): Json<CreateSessionInput>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.registry.create(input).await?))
}
```

WebSocket 流式接口也应该只是把 core 产出的 payload 发到 socket：

```rust
chat::run_chat_stream(registry, session_id, message, move |payload| {
    let payload_tx = payload_tx.clone();
    async move {
        payload_tx
            .send(payload)
            .map_err(|e| AppError::Internal(e.to_string()))
    }
})
.await;
```

## 通信方式

### Tauri 桌面模式

前端通过 Tauri IPC 调用 Rust command：

```text
WebView -> Tauri IPC -> apps/echo-tauri/src-tauri -> apps/echo-app-core
```

流式消息通过 Tauri event 返回：

```text
apps/echo-app-core -> StreamPayload -> app.emit("echo://agent/stream/{session_id}") -> WebView
```

这条路径不经过 HTTP，不监听端口，也不占用 TCP 端口。

### Web/移动端模式

前端通过 HTTP/WebSocket 调用本地 server：

```text
Browser / Mobile -> HTTP/WebSocket -> apps/echo-http-server -> apps/echo-app-core
```

页面和 API 使用同一个端口。启动时 server 生成 token，并打印带 token 的访问地址。手机扫码访问后，前端可把 token 写入 cookie，后续请求通过 cookie、Bearer 或 query token 鉴权。

## 新功能开发规则

新增任何功能时，按下面顺序开发。

1. 先在 `apps/echo-app-core` 设计输入输出类型和核心函数。

   核心函数应该表达真实业务含义，例如 `create_xxx`、`update_xxx`、`run_xxx`、`collect_xxx`。如果需要调用 `echo-agents` 或底层 agent/tool/state crate，调用代码也放在这里。

2. 在 `apps/echo-app-core` 写或迁移测试。

   只要测试不依赖 Tauri IPC、HTTP、WebSocket 协议细节，就应该放在 core。真实模型或真实工具链测试可以保留 `#[ignore]`，但仍应归属 core。

3. 在 `apps/echo-tauri` 新增对应 Tauri command。

   command 只负责参数接收、状态提取、调用 core、返回结果。流式接口只负责 `app.emit`。

4. 在 `apps/echo-http-server` 新增对应 HTTP 或 WebSocket 接口。

   route 只负责协议层输入输出、鉴权和序列化。业务逻辑必须调用 core。

5. 在前端 transport 层同时接入两种环境。

   Tauri 环境调用 IPC；浏览器环境调用 HTTP/WebSocket。前端业务组件不应该直接关心当前 transport。

## 判断代码应该放在哪里

可以用下面几个问题判断：

- 这段代码是否直接调用 `echo-agent`、`echo-agents`、工具、模型、状态或持久化？如果是，放 `echo-app-core`。
- 这段代码是否描述应用业务规则？如果是，放 `echo-app-core`。
- 这段代码是否只是 Tauri command、`State`、`AppHandle.emit`？如果是，放 `echo-tauri`。
- 这段代码是否只是 HTTP route、middleware、cookie、CORS、WebSocket frame？如果是，放 `echo-http-server`。
- 两个入口都需要同一段逻辑？先抽到 `echo-app-core`，两个入口分别薄薄调用。

## 目标

这个架构的目标是让新增功能只实现一次业务逻辑，同时支持两种客户端：

- 桌面应用保留 Tauri IPC 的本地体验。
- 网页和手机可以通过 HTTP/WebSocket 连接同一个后端。

因此，`echo-tauri` 和 `echo-http-server` 可以同时存在，但它们不应该成为两套业务实现。它们只是两种协议外壳，真正的应用能力都应该沉到 `echo-app-core`。
