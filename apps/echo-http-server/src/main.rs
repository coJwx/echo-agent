use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::Request;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::{Method, StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router, body::Body};
use echo_app_core::chat;
use echo_app_core::error::AppError;
use echo_app_core::events::StreamPayload;
use echo_app_core::provider::{self, ProviderConfig, ProviderModelInput};
use echo_app_core::state::{
    AgentRegistry, AgentRegistryPaths, CreateSessionInput, UpdateSessionModelInput,
};
use echo_core::utils::paths::root_agent_dir;
use futures_util::{SinkExt, StreamExt};
use include_dir::{Dir, include_dir};
use qrcode::QrCode;
use qrcode::render::svg;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::{AllowOrigin, CorsLayer};
use uuid::Uuid;

const TOKEN_TTL_MS: u64 = 7 * 24 * 60 * 60 * 1000;
static EMBEDDED_WEB_DIST: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../echo-tauri/dist");

#[derive(Clone)]
struct AppState {
    registry: Arc<AgentRegistry>,
    token: Arc<RwLock<TokenRecord>>,
    token_path: Arc<PathBuf>,
    web_port: Arc<str>,
    static_dir: Option<Arc<PathBuf>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TokenRecord {
    token: String,
    created_at_ms: u64,
}

#[derive(Debug, Serialize)]
struct AccessTokenInfo {
    token: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    local_url: String,
    network_url: Option<String>,
    qr_svg: String,
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
    let app_data_dir = app_data_dir()?;
    let token_path = app_data_dir.join("access-token.json");
    let token = load_or_create_token(&token_path)?;
    let web_port = web_port();
    let registry =
        Arc::new(AgentRegistry::new(AgentRegistryPaths::from_app_data_dir(app_data_dir)).await?);

    let static_dir = web_dist_dir();
    let app = router(
        AppState {
            registry,
            token: Arc::new(RwLock::new(token.clone())),
            token_path: Arc::new(token_path),
            web_port: Arc::from(web_port.as_str()),
            static_dir: static_dir.map(Arc::new),
        },
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "echo HTTP/WebSocket server listening");
    print_frontend_urls(&web_port, &token.token);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/api/sessions", get(list_sessions).post(create_session))
        .route("/api/sessions/:session_id", delete(delete_session))
        .route("/api/sessions/:session_id/model", post(update_session_model))
        .route("/api/sessions/:session_id/history", get(session_history))
        .route("/api/sessions/:session_id/stream", get(session_stream))
        .route("/api/provider/config", get(provider_config_get))
        .route("/api/provider/models", post(provider_model_save))
        .route("/api/access-token", get(access_token_get))
        .route("/api/access-token/refresh", post(access_token_refresh))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_token));

    Router::new()
        .route("/api/health", get(health))
        .merge(protected)
        .fallback(static_asset)
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
    let authorized = {
        let token = state.token.read().await;
        request_token(&request).as_deref() == Some(token.token.as_str())
    };

    if authorized {
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

async fn static_asset(State(state): State<AppState>, uri: Uri) -> Response {
    let request_path = static_request_path(uri.path());
    if let Some(static_dir) = state.static_dir.as_deref()
        && let Some(response) = file_response_from_disk(static_dir, &request_path).await
    {
        return response;
    }

    file_response_from_embed(&request_path)
}

fn static_request_path(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        "index.html".to_string()
    } else {
        path.replace('\\', "/")
    }
}

async fn file_response_from_disk(static_dir: &PathBuf, request_path: &str) -> Option<Response> {
    let path = static_dir.join(request_path);
    let path = if tokio::fs::metadata(&path)
        .await
        .map(|meta| meta.is_file())
        .unwrap_or(false)
    {
        path
    } else {
        static_dir.join("index.html")
    };

    let bytes = tokio::fs::read(&path).await.ok()?;
    let mime = mime_guess::from_path(&path).first_or_octet_stream();
    Some(response_with_bytes(bytes, mime.as_ref()))
}

fn file_response_from_embed(request_path: &str) -> Response {
    let file = EMBEDDED_WEB_DIST
        .get_file(request_path)
        .or_else(|| EMBEDDED_WEB_DIST.get_file("index.html"));

    if let Some(file) = file {
        let mime = mime_guess::from_path(file.path()).first_or_octet_stream();
        response_with_bytes(file.contents().to_vec(), mime.as_ref())
    } else {
        (StatusCode::NOT_FOUND, "embedded frontend is missing").into_response()
    }
}

fn response_with_bytes(bytes: Vec<u8>, content_type: &str) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap_or_else(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to build static response: {e}"),
            )
                .into_response()
        })
}

async fn access_token_get(
    State(state): State<AppState>,
) -> Result<Json<AccessTokenInfo>, ApiError> {
    let token = state.token.read().await;
    Ok(Json(access_token_info(&state, &token)?))
}

async fn access_token_refresh(
    State(state): State<AppState>,
) -> Result<Json<AccessTokenInfo>, ApiError> {
    let mut token = state.token.write().await;
    *token = new_token_record();
    persist_token(&state.token_path, &token).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(access_token_info(&state, &token)?))
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

async fn update_session_model(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    Json(input): Json<UpdateSessionModelInput>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.registry.update_model(&session_id, input.model).await?))
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

    let path = root_agent_dir().join("echo-http-server");
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn web_dist_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ECHO_WEB_DIST")
        && !path.trim().is_empty()
    {
        let path = PathBuf::from(path);
        return Some(if path.is_absolute() {
            path
        } else {
            std::env::current_dir().ok()?.join(path)
        });
    }

    None
}

fn load_or_create_token(path: &PathBuf) -> anyhow::Result<TokenRecord> {
    if let Ok(token) = std::env::var("ECHO_HTTP_TOKEN")
        && !token.trim().is_empty()
    {
        let record = TokenRecord {
            token,
            created_at_ms: now_ms(),
        };
        persist_token(path, &record)?;
        return Ok(record);
    }

    if let Ok(text) = std::fs::read_to_string(path)
        && let Ok(record) = serde_json::from_str::<TokenRecord>(&text)
        && !record.token.trim().is_empty()
        && now_ms().saturating_sub(record.created_at_ms) < TOKEN_TTL_MS
    {
        return Ok(record);
    }

    let record = new_token_record();
    persist_token(path, &record)?;
    Ok(record)
}

fn new_token_record() -> TokenRecord {
    TokenRecord {
        token: Uuid::new_v4().simple().to_string(),
        created_at_ms: now_ms(),
    }
}

fn persist_token(path: &PathBuf, record: &TokenRecord) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = serde_json::to_string_pretty(record)?;
    std::fs::write(path, text)?;
    Ok(())
}

fn access_token_info(state: &AppState, record: &TokenRecord) -> Result<AccessTokenInfo, ApiError> {
    let local_url = frontend_url("localhost", &state.web_port, &record.token);
    let network_url =
        local_network_ip().map(|ip| frontend_url(&ip, &state.web_port, &record.token));
    let code = QrCode::new(record.token.as_bytes())
        .map_err(|e| ApiError(AppError::Internal(e.to_string())))?;
    let qr_svg = code
        .render::<svg::Color>()
        .min_dimensions(256, 256)
        .dark_color(svg::Color("#111111"))
        .light_color(svg::Color("#ffffff"))
        .build();

    Ok(AccessTokenInfo {
        token: record.token.clone(),
        created_at_ms: record.created_at_ms,
        expires_at_ms: record.created_at_ms.saturating_add(TOKEN_TTL_MS),
        local_url,
        network_url,
        qr_svg,
    })
}

fn frontend_url(host: &str, port: &str, token: &str) -> String {
    format!("http://{host}:{port}?token={token}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

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

fn print_frontend_urls(port: &str, token: &str) {
    println!("➜  Local:   {}", frontend_url("localhost", port, token));
    if let Some(ip) = local_network_ip() {
        println!("➜  Network: {}", frontend_url(&ip, port, token));
    }
}

fn web_port() -> String {
    std::env::var("ECHO_WEB_PORT")
        .or_else(|_| {
            std::env::var("ECHO_HTTP_BIND")
                .ok()
                .and_then(|bind| bind.rsplit_once(':').map(|(_, port)| port.to_string()))
                .ok_or(std::env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| "4399".to_string())
}

fn local_network_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
