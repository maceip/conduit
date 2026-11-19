//! The broker: the middle of a tunnel.
//!
//! The broker routes frames between the two ends of a tunnel, holds the
//! user's grant, and enforces it.  It does not read payloads: they are
//! sealed between the two ends, and the broker sees the envelope only.
//!
//! The registry is in memory for now — the bootstrap has one process and
//! one grant store.  Everything that has to survive a restart is behind
//! the `Registry` trait, so the store can be replaced without touching
//! the relay.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use conduit_protocol::{
    ErrorCode, Failure, Frame, FrameType, Grant, ProtocolError, Replay, SchemaId, Scope,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

mod devices;
mod registry;
mod relay;

pub use devices::{Device, Devices};
pub use registry::{InMemoryRegistry, Registry};
pub use relay::Tunnel;

/// The state the server shares.
#[derive(Clone)]
pub struct AppState {
    /// The registry of surfaces and grants.
    pub registry: Arc<dyn Registry>,
    /// The tunnels that are open, by surface.
    pub tunnels: Arc<Mutex<HashMap<SchemaId, Tunnel>>>,
    /// The codes the user approves.
    pub devices: Arc<Devices>,
    /// The secret the extension presents, for the bootstrap.
    pub extension_secret: String,
}

/// The parameters of a tunnel request.
#[derive(Debug, Clone, Deserialize)]
pub struct Params {
    /// `extension` or `client`.
    pub role: String,
    /// The surface, for an extension.
    pub surface: Option<String>,
    /// The secret of the extension.
    pub secret: Option<String>,
    /// The token of the client.
    pub token: Option<String>,
    /// The instance of the extension, for the grant.
    pub instance: Option<String>,
}

/// The broker's HTTP surface.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/tunnel", get(tunnel))
        .route("/v1/health", get(health))
        .route("/v1/device/code", post(ask_for_a_code))
        .route("/v1/device/approve", post(approve_a_code))
        .route("/v1/device/secret", post(the_secret))
        .route(
            "/v1/device/approve-page",
            get(the_approval_page).post(approve_from_the_page),
        )
        .with_state(state)
}

/// The body of a request for a code.
#[derive(Debug, Clone, Deserialize)]
pub struct CodeRequest {
    /// The surface the extension wants to offer.
    pub surface: String,
    /// The scopes it asks for.
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// The body of a request for the secret.
#[derive(Debug, Clone, Deserialize)]
pub struct SecretRequest {
    /// The code the extension was given.
    pub code: String,
    /// The instance of the extension, for the grant.
    #[serde(default)]
    pub instance: String,
}

/// A code, and what the user does with it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodeReply {
    /// The code itself.
    pub code: String,
    /// Where the user approves it.
    pub url: String,
    /// How long it lives, in seconds.
    pub expires_in: u64,
}

/// Ask for a code.
async fn ask_for_a_code(
    State(state): State<AppState>,
    body: axum::Json<CodeRequest>,
) -> axum::Json<CodeReply> {
    let surface = SchemaId::parse(&body.surface)
        .unwrap_or_else(|| SchemaId::new("dev.conduit.browser.tabs", 1));
    let scopes = body
        .scopes
        .iter()
        .filter_map(|scope| scope.rsplit_once('.'))
        .map(|(namespace, verb)| Scope::new(namespace, verb))
        .collect();
    let code = state.devices.ask(surface, scopes).await;
    axum::Json(CodeReply {
        code: code.clone(),
        url: format!("/v1/device/approve-page?code={code}"),
        expires_in: devices::CODE_LIFETIME_SECONDS,
    })
}

/// The body of an approval.
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalRequest {
    /// The code the user is approving.
    pub code: String,
}

/// The user approves a code.
async fn approve_a_code(
    State(state): State<AppState>,
    body: axum::Json<ApprovalRequest>,
) -> impl axum::response::IntoResponse {
    match state.devices.approve(&body.code).await {
        Some(device) => (
            axum::http::StatusCode::OK,
            format!("approved {}", device.surface),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            String::from("no such code, or it expired"),
        ),
    }
}

/// The secret of an approved code.
async fn the_secret(
    State(state): State<AppState>,
    body: axum::Json<SecretRequest>,
) -> impl axum::response::IntoResponse {
    match state.devices.secret(&body.code, &body.instance).await {
        Some((secret, grant)) => {
            state.registry.put_grant(grant).await;
            (axum::http::StatusCode::OK, secret)
        }
        None => (
            axum::http::StatusCode::FORBIDDEN,
            String::from("code not approved yet, or expired"),
        ),
    }
}

/// The page the user approves a code in.
async fn the_approval_page(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
) -> impl axum::response::IntoResponse {
    let code = query.get("code").cloned().unwrap_or_default();
    let device = state.devices.device(&code).await;
    let body = match device {
        Some(device) => format!(
            "<h1>approve this extension</h1><p><strong>{}</strong> asks for these scopes:</p><ul>{}</ul><form method=\"post\" action=\"/v1/device/approve-page\"><input type=\"hidden\" name=\"code\" value=\"{}\"><button type=\"submit\">approve</button></form>",
            escape(&device.surface.to_string()),
            device
                .scopes
                .iter()
                .map(|scope| format!("<li>{}</li>", escape(&scope.to_string())))
                .collect::<Vec<_>>()
                .join(""),
            escape(&code)
        ),
        None => "<h1>no such code</h1><p>expired, or never issued.</p>".to_string(),
    };
    (
        axum::http::StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>approve</title></head><body>{body}</body></html>"
        ),
    )
}

/// The user presses approve on the page.
///
/// The page is a plain html form, so the code arrives form-encoded; the
/// answer is a page the user can read, not a status for a script.
async fn approve_from_the_page(
    State(state): State<AppState>,
    axum::Form(body): axum::Form<ApprovalRequest>,
) -> impl axum::response::IntoResponse {
    let (status, body) = match state.devices.approve(&body.code).await {
        Some(device) => (
            axum::http::StatusCode::OK,
            format!(
                "<h1>approved</h1><p><strong>{}</strong> can open its tunnel now. you can close this tab.</p>",
                escape(&device.surface.to_string())
            ),
        ),
        None => (
            axum::http::StatusCode::NOT_FOUND,
            "<h1>no such code</h1><p>expired, or never issued.</p>".to_string(),
        ),
    };
    (
        status,
        [("content-type", "text/html; charset=utf-8")],
        format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>approve</title></head><body>{body}</body></html>"
        ),
    )
}

/// Text made safe for html: the scopes on the page come from whoever
/// asked for the code.
fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// The health of the broker.
async fn health() -> &'static str {
    "ok"
}

/// Open a tunnel.
async fn tunnel(
    upgrade: WebSocketUpgrade,
    Query(params): Query<Params>,
    State(state): State<AppState>,
) -> Response {
    upgrade.on_upgrade(move |socket| handle(socket, state, params))
}

/// Send an error frame and give up on the socket.
async fn refuse(socket: &mut WebSocket, code: ErrorCode, message: &str) {
    let _ = socket
        .send(Message::Text(error_frame(code, message, None).into()))
        .await;
    let _ = socket.send(Message::Close(None)).await;
}

/// Carry one end of a tunnel.
async fn handle(mut socket: WebSocket, state: AppState, params: Params) {
    let surface = match params.surface.as_deref().and_then(SchemaId::parse) {
        Some(surface) => surface,
        None => {
            refuse(&mut socket, ErrorCode::SurfaceUnknown, "no surface named").await;
            return;
        }
    };

    // The first frame of an end is the frame that says who it is.
    let hello = match first_hello(&mut socket).await {
        Some(hello) => hello,
        None => return,
    };

    match params.role.as_str() {
        "extension" => {
            // The secret is what the user approved, and it carries the
            // instance it was approved for; the bootstrap's own secret is
            // there for the tests.
            let secret = params.secret.as_deref().unwrap_or_default();
            let instance = match state.devices.credential(secret).await {
                Some((surface_of_the_secret, instance)) => {
                    if surface_of_the_secret != surface {
                        refuse(
                            &mut socket,
                            ErrorCode::Unauthorized,
                            "secret belongs to another surface",
                        )
                        .await;
                        return;
                    }
                    instance
                }
                None if !state.extension_secret.is_empty() && secret == state.extension_secret => {
                    params
                        .instance
                        .as_deref()
                        .unwrap_or("bootstrap")
                        .to_string()
                }
                None => {
                    refuse(&mut socket, ErrorCode::Unauthorized, "not authorized").await;
                    return;
                }
            };
            let grant = state.registry.grant_for(&surface, &instance).await;
            let tunnel = Tunnel::new(Arc::clone(&state.registry), grant);
            tunnel.set_extension_hello(hello).await;
            let Some(receiver) = tunnel.take_for_extension().await else {
                return;
            };
            // A second extension for the same surface takes over; the
            // tunnel it replaces closes, and its client with it.
            if let Some(old) = state
                .tunnels
                .lock()
                .await
                .insert(surface.clone(), tunnel.clone())
            {
                old.close();
            }
            welcome(&mut socket, &surface).await;
            relay::serve_end(socket, tunnel.clone(), receiver, relay::End::Extension).await;
            let mut tunnels = state.tunnels.lock().await;
            if tunnels
                .get(&surface)
                .is_some_and(|current| current.same(&tunnel))
            {
                tunnels.remove(&surface);
            }
        }
        "client" => {
            let token = params.token.as_deref().unwrap_or_default();
            if !state
                .registry
                .surfaces_for_token(token)
                .await
                .contains(&surface)
            {
                refuse(
                    &mut socket,
                    ErrorCode::Unauthorized,
                    "token does not cover that surface",
                )
                .await;
                return;
            }
            let tunnel = state.tunnels.lock().await.get(&surface).cloned();
            let Some(tunnel) = tunnel else {
                refuse(
                    &mut socket,
                    ErrorCode::SurfaceUnknown,
                    "no extension is connected for that surface",
                )
                .await;
                return;
            };
            let Some(receiver) = tunnel.take_for_client().await else {
                refuse(
                    &mut socket,
                    ErrorCode::Unauthorized,
                    "another client holds this tunnel",
                )
                .await;
                return;
            };
            welcome(&mut socket, &surface).await;
            if let Some(their_hello) = tunnel.extension_hello().await {
                let text = serde_json::to_string(&their_hello).unwrap_or_default();
                let _ = socket.send(Message::Text(text.into())).await;
            }
            let _ = tunnel.to_extension.try_send(hello);
            let receiver =
                relay::serve_end(socket, tunnel.clone(), receiver, relay::End::Client).await;
            tunnel.give_back_for_client(receiver).await;
        }
        _ => {
            refuse(&mut socket, ErrorCode::Unauthorized, "no such role").await;
        }
    }
}

/// The first frame of an end: the hello.
///
/// The hello carries the public key of the end, so the frame itself is
/// kept: the broker hands it to the other end, which needs it to agree
/// the key of the tunnel.
async fn first_hello(socket: &mut WebSocket) -> Option<Frame> {
    while let Some(Ok(message)) = socket.recv().await {
        if let Message::Text(text) = message {
            if let Ok(frame) = serde_json::from_str::<Frame>(&text) {
                if frame.kind == FrameType::Hello {
                    return Some(frame);
                }
            }
        }
    }
    None
}

/// The frame that says the broker is ready for this end.
async fn welcome(socket: &mut WebSocket, surface: &SchemaId) {
    let frame = Frame {
        kind: FrameType::Welcome,
        schema: SchemaId::new("dev.conduit.welcome", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: Some(serde_json::json!({ "surface": surface.to_string(), "seq": 0 })),
    };
    let text = serde_json::to_string(&frame).unwrap_or_default();
    let _ = socket.send(Message::Text(text.into())).await;
}

/// An error frame: the envelope of `dev.conduit.error/v1`, with the
/// error as its payload. errors aren't sealed; the broker writes them.
pub fn error_frame(code: ErrorCode, message: &str, scope: Option<String>) -> String {
    let error = ProtocolError {
        code,
        message: message.to_string(),
        reference: None,
        scope,
    };
    serde_json::to_string(&Frame {
        kind: FrameType::Error,
        schema: SchemaId::new("dev.conduit.error", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: Some(serde_json::to_value(&error).unwrap_or_else(|_| serde_json::json!({}))),
    })
    .unwrap_or_else(|_| "{}".to_string())
}

/// The failure of a frame, as an error frame.
pub fn failure_frame(failure: &Failure) -> String {
    error_frame(
        failure.code(),
        &failure.to_string(),
        failure.scope().map(|scope| scope.to_string()),
    )
}

/// A ring of the frames of a tunnel, for resume.
pub type Ring = Replay;

/// The scopes a client may ask for, and the ones the grant covers.
pub fn scopes_the_grant_allows(grant: &Grant, wanted: &[Scope]) -> Result<(), Vec<Scope>> {
    let missing = grant.missing(wanted);
    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing)
    }
}

/// Seed the registry from a bootstrap file.
///
/// The file says which surfaces a deployment offers, which grants exist
/// for them, and which token a client may use:
///
/// ```json
/// {
///   "token": "bootstrap-token",
///   "surfaces": [
///     {"id": "dev.conduit.vscode.tabs/v1",
///      "schemas": ["dev.conduit.vscode.tab/v1"],
///      "scopes": ["vscode.tabs.read"]}
///   ],
///   "grants": [
///     {"surface": "dev.conduit.vscode.tabs/v1",
///      "instance": "bootstrap",
///      "scopes": ["vscode.tabs.read"]}
///   ]
/// }
/// ```
pub async fn bootstrap(registry: &InMemoryRegistry, path: &str) -> std::io::Result<()> {
    let text = std::fs::read_to_string(path)?;
    let file: serde_json::Value = serde_json::from_str(&text)
        .map_err(|error| std::io::Error::other(format!("the bootstrap is not JSON ({error})")))?;

    let mut surface_ids = Vec::new();
    if let Some(surfaces) = file.get("surfaces").and_then(|s| s.as_array()) {
        for surface in surfaces {
            let id = surface
                .get("id")
                .and_then(|id| id.as_str())
                .and_then(conduit_protocol::SchemaId::parse)
                .ok_or_else(|| {
                    std::io::Error::other("a surface of the bootstrap has no usable id")
                })?;
            let schemas = surface
                .get("schemas")
                .and_then(|s| s.as_array())
                .map(|schemas| {
                    schemas
                        .iter()
                        .filter_map(|schema| schema.as_str())
                        .filter_map(conduit_protocol::SchemaId::parse)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let scopes = scope_strings(surface.get("scopes"));
            registry
                .put_surface(conduit_protocol::Surface {
                    id: id.clone(),
                    schemas,
                    scopes,
                    schemes: vec!["p256-aes256gcm/v1".to_string()],
                })
                .await;
            surface_ids.push(id);
        }
    }

    if let Some(grants) = file.get("grants").and_then(|g| g.as_array()) {
        for grant in grants {
            let surface = grant
                .get("surface")
                .and_then(|surface| surface.as_str())
                .and_then(conduit_protocol::SchemaId::parse)
                .ok_or_else(|| {
                    std::io::Error::other("a grant of the bootstrap has no usable surface")
                })?;
            let instance = grant
                .get("instance")
                .and_then(|instance| instance.as_str())
                .unwrap_or("bootstrap")
                .to_string();
            registry
                .put_grant(conduit_protocol::Grant {
                    surface,
                    instance,
                    scopes: scope_strings(grant.get("scopes")),
                    schemes: vec!["p256-aes256gcm/v1".to_string()],
                })
                .await;
        }
    }

    if let Some(token) = file.get("token").and_then(|token| token.as_str()) {
        registry.put_token(token, surface_ids).await;
    }
    Ok(())
}

/// The scopes of a bootstrap entry, read as strings.
fn scope_strings(value: Option<&serde_json::Value>) -> Vec<conduit_protocol::Scope> {
    value
        .and_then(|value| value.as_array())
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| scope.as_str())
                .filter_map(conduit_protocol::Scope::parse)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}
