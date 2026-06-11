use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::Request;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use echo_app_core::chat;
use echo_app_core::error::AppError;
use echo_app_core::events::StreamPayload;
use echo_app_core::provider::{self, ProviderConfig, ProviderModelInput};
use echo_app_core::state::{AgentRegistry, AgentRegistryPaths, CreateSessionInput};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    registry: Arc<AgentRegistry>,
    token: Arc<str>,
}

#[derive(Debug)]
struct ApiError(AppError);

impl From<AppError> for ApiError {
    fn from(value: AppError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            AppError::SessionNotFound(_) => StatusCode::NOT_FOUND,
            AppError::Config(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(serde_json::json!({ "error": self.0.to_string() }));
        (status, body).into_response()
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientWsMessage {
    Chat { message: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    bootstrap_workspace_root();
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,echo_http_server=debug".into()),
        )
        .init();

    let bind = std::env::var("ECHO_HTTP_BIND").unwrap_or_else(|_| "127.0.0.1:4399".to_string());
    let addr: SocketAddr = bind.parse()?;
    let token = std::env::var("ECHO_HTTP_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    let app_data_dir = app_data_dir()?;
    let registry =
        Arc::new(AgentRegistry::new(AgentRegistryPaths::from_app_data_dir(app_data_dir)).await?);

    let static_dir = web_dist_dir()?;
    let app = router(
        AppState {
            registry,
            token: Arc::from(token.as_str()),
        },
        static_dir,
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "echo HTTP/WebSocket server listening");
    print_frontend_urls(&token);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn router(state: AppState, static_dir: PathBuf) -> Router {
    let protected = Router::new()
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:session_id", delete(delete_session))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/api/sessions/:session_id/stream", get(session_stream))
        .route("/api/provider/config", get(provider_config_get))
        .route("/api/provider/models", post(provider_model_save))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/api/health", get(health))
        .merge(protected)
        .fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html"))),
        )
        .layer(cors_layer())
        .with_state(state)
}

fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_credentials(true)
}

async fn require_token(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if request_token(&request).as_deref() == Some(state.token.as_ref()) {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn request_token(request: &Request) -> Option<String> {
    bearer_token(request)
        .or_else(|| cookie_token(request))
        .or_else(|| query_token(request))
}

fn bearer_token(request: &Request) -> Option<String> {
    let value = request
        .headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?;
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
}

fn cookie_token(request: &Request) -> Option<String> {
    let value = request.headers().get(header::COOKIE)?.to_str().ok()?;
    value.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        (name == "echo_http_token" && !value.is_empty()).then(|| value.to_string())
    })
}

fn query_token(request: &Request) -> Option<String> {
    request.uri().query()?.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name == "token" && !value.is_empty()).then(|| value.to_string())
    })
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "ok": true }))
}

async fn create_session(
    State(state): State<AppState>,
    Json(input): Json<CreateSessionInput>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.registry.create(input).await?))
}

async fn list_sessions(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.registry.list().await))
}

async fn delete_session(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state.registry.delete(&session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn session_history(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.registry.history(&session_id).await?))
}

async fn provider_config_get() -> Result<Json<ProviderConfig>, ApiError> {
    Ok(Json(provider::provider_config_get().await?))
}

async fn provider_model_save(
    Json(input): Json<ProviderModelInput>,
) -> Result<Json<ProviderConfig>, ApiError> {
    Ok(Json(provider::provider_model_save(input).await?))
}

async fn session_stream(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(state, session_id, socket))
}

async fn handle_socket(state: AppState, session_id: String, socket: WebSocket) {
    let (mut sender, mut receiver) = socket.split();
    while let Some(Ok(message)) = receiver.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        match serde_json::from_str::<ClientWsMessage>(&text) {
            Ok(ClientWsMessage::Chat { message }) => {
                let (payload_tx, mut payload_rx) = tokio::sync::mpsc::unbounded_channel();
                let registry = Arc::clone(&state.registry);
                let session_id = session_id.clone();
                tokio::spawn(async move {
                    chat::run_chat_stream(registry, session_id, message, move |payload| {
                        let payload_tx = payload_tx.clone();
                        async move {
                            payload_tx
                                .send(payload)
                                .map_err(|e| AppError::Internal(e.to_string()))
                        }
                    })
                    .await;
                });

                while let Some(payload) = payload_rx.recv().await {
                    let done = matches!(payload, StreamPayload::Done { .. });
                    if send_payload(&mut sender, &payload).await.is_err() || done {
                        break;
                    }
                }
            }
            Err(e) => {
                let payload = StreamPayload::Error {
                    source: "websocket".to_string(),
                    message: format!("invalid client message: {e}"),
                };
                let _ = send_payload(&mut sender, &payload).await;
            }
        }
    }
}

async fn send_payload<S>(sender: &mut S, payload: &StreamPayload) -> Result<(), S::Error>
where
    S: futures_util::Sink<Message> + Unpin,
{
    let text = serde_json::to_string(payload).unwrap_or_else(|e| {
        serde_json::json!({
            "kind": "error",
            "source": "serialize",
            "message": e.to_string(),
        })
        .to_string()
    });
    sender.send(Message::Text(text)).await
}

fn app_data_dir() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("ECHO_HTTP_DATA_DIR")
        && !path.trim().is_empty()
    {
        let path = PathBuf::from(path);
        std::fs::create_dir_all(&path)?;
        return Ok(path);
    }

    let path = std::env::current_dir()?.join(".echo-http-server");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn web_dist_dir() -> anyhow::Result<PathBuf> {
    if let Ok(path) = std::env::var("ECHO_WEB_DIST")
        && !path.trim().is_empty()
    {
        let path = PathBuf::from(path);
        return Ok(if path.is_absolute() {
            path
        } else {
            std::env::current_dir()?.join(path)
        });
    }

    Ok(std::env::current_dir()?
        .join("apps")
        .join("echo-tauri")
        .join("dist"))
}

fn bootstrap_workspace_root() {
    let Ok(mut cur) = std::env::current_dir() else {
        return;
    };
    for _ in 0..8 {
        let candidate = cur.join("echo-agent-models.yaml");
        if candidate.is_file() {
            let _ = std::env::set_current_dir(&cur);
            unsafe {
                std::env::set_var("ECHO_AGENT_MODELS_CONFIG", candidate);
            }
            return;
        }
        if !cur.pop() {
            break;
        }
    }
}

fn print_frontend_urls(token: &str) {
    let port = std::env::var("ECHO_WEB_PORT")
        .or_else(|_| {
            std::env::var("ECHO_HTTP_BIND")
                .ok()
                .and_then(|bind| bind.rsplit_once(':').map(|(_, port)| port.to_string()))
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "4399".to_string());
    println!("➜  Local:   http://localhost:{port}?token={token}");
    if let Some(ip) = local_network_ip() {
        println!("➜  Network: http://{ip}:{port}?token={token}");
    }
}

fn local_network_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
