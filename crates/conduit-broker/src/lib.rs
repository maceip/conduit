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
        .route("/v1/device/approve-page", get(the_approval_page))
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
            device.surface,
            device
                .scopes
                .iter()
                .map(|scope| format!("<li>{}</li>", scope))
                .collect::<Vec<_>>()
                .join(""),
            code
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

/// Carry one end of a tunnel.
async fn handle(mut socket: WebSocket, state: AppState, params: Params) {
    let surface = match params.surface.as_deref().and_then(SchemaId::parse) {
        Some(surface) => surface,
        None => return,
    };

    // The first frame of an end is the frame that says who it is.
    let hello = match first_hello(&mut socket).await {
        Some(hello) => hello,
        None => return,
    };

    match params.role.as_str() {
        "extension" => {
            // The secret is what the user approved; the bootstrap's
            // own secret is there for the tests.
            let secret = params.secret.as_deref().unwrap_or_default();
            let known = state.devices.surface_of(secret).await;
            if known.is_none() && secret != state.extension_secret {
                let _ = socket
                    .send(Message::Text(
                        error_frame(ErrorCode::Unauthorized, "not authorized").into(),
                    ))
                    .await;
                return;
            }
            if let Some(surface_of_the_secret) = known {
                if surface_of_the_secret != surface {
                    let _ = socket
                        .send(Message::Text(
                            error_frame(
                                ErrorCode::Unauthorized,
                                "secret belongs to another surface",
                            )
                            .into(),
                        ))
                        .await;
                    return;
                }
            }
            let instance = params.instance.as_deref().unwrap_or("bootstrap");
            let grant = state.registry.grant_for(&surface, instance).await;
            let tunnel = Tunnel::new(Arc::clone(&state.registry), grant);
            state
                .tunnels
                .lock()
                .await
                .insert(surface.clone(), tunnel.clone());
            let receiver = match tunnel.take_for_extension().await {
                Some(receiver) => receiver,
                None => return,
            };
            welcome(&mut socket, &surface).await;
            let _ = tunnel.to_client.send(hello.clone()).await;
            relay::serve_extension(socket, tunnel, receiver).await;
        }
        "client" => {
            let token = match params.token.as_deref() {
                Some(token) => token.to_string(),
                None => return,
            };
            let tunnel = state
                .tunnels
                .lock()
                .await
                .get(&surface)
                .cloned()
                .unwrap_or_else(|| {
                    Tunnel::new(
                        Arc::clone(&state.registry),
                        Grant {
                            surface: surface.clone(),
                            instance: "bootstrap".to_string(),
                            scopes: Vec::new(),
                            schemes: Vec::new(),
                        },
                    )
                });
            let receiver = match tunnel.take_for_client().await {
                Some(receiver) => receiver,
                None => return,
            };
            welcome(&mut socket, &surface).await;
            let _ = tunnel.to_extension.send(hello.clone()).await;
            relay::serve_client(socket, tunnel, receiver, token).await;
        }
        _ => {}
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
        payload: None,
    };
    let mut frame = frame;
    frame.channel = surface.to_string();
    let text = serde_json::to_string(&frame).unwrap_or_default();
    let _ = socket.send(Message::Text(text.into())).await;
}

/// The frame that says an error.
fn error_frame(code: ErrorCode, message: &str) -> String {
    let error = ProtocolError {
        code,
        message: message.to_string(),
        reference: None,
        scope: None,
    };
    serde_json::to_string(&error).unwrap_or_else(|_| "{}".to_string())
}

/// The failure of a frame, as the code the wire carries.
pub fn failure_frame(failure: &Failure) -> String {
    let mut error = ProtocolError {
        code: failure.code(),
        message: failure.to_string(),
        reference: None,
        scope: None,
    };
    if let Some(scope) = failure.scope() {
        error.scope = Some(scope.to_string());
    }
    serde_json::to_string(&error).unwrap_or_else(|_| "{}".to_string())
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
