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
use axum::routing::get;
use axum::Router;
use conduit_protocol::{
    ErrorCode, Failure, Frame, FrameType, Grant, ProtocolError, Replay, SchemaId, Scope,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

mod registry;
mod relay;

pub use registry::{InMemoryRegistry, Registry};
pub use relay::Tunnel;

/// The state the server shares.
#[derive(Clone)]
pub struct AppState {
    /// The registry of surfaces and grants.
    pub registry: Arc<dyn Registry>,
    /// The tunnels that are open, by surface.
    pub tunnels: Arc<Mutex<HashMap<SchemaId, Tunnel>>>,
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
}

/// The broker's HTTP surface.
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/v1/tunnel", get(tunnel))
        .route("/v1/health", get(health))
        .with_state(state)
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
            if params.secret.as_deref() != Some(state.extension_secret.as_str()) {
                let _ = socket
                    .send(Message::Text(
                        error_frame(ErrorCode::Unauthorized, "not authorized").into(),
                    ))
                    .await;
                return;
            }
            let grant = state.registry.grant_for(&surface, "bootstrap").await;
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
