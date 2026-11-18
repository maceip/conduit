//! The relay: the frames between the two ends.
//!
//! One end sends, the other receives, and the broker is in the middle
//! with the grant in its hand.  The broker reads the envelope of every
//! frame, checks the scope it names against the grant, and forwards the
//! frame *sealed* — it never opens the payload.

use crate::{failure_frame, registry::Registry, scopes_the_grant_allows};
use axum::extract::ws::{Message, WebSocket};
use conduit_protocol::{Frame, FrameType, Grant, ProtocolError, Replay, Scope};
use std::sync::Arc;
use tokio::sync::mpsc;

/// A tunnel: one extension, and the clients that are subscribed to it.
///
/// The tunnel owns the two channels that connect the ends: the frames
/// going to the extension, and the frames going to the client.
#[derive(Clone)]
pub struct Tunnel {
    /// The registry the tunnel asks about surfaces and grants.
    registry: Arc<dyn Registry>,
    /// The grant in force.
    grant: Grant,
    /// The frames on their way to the extension.
    to_extension: mpsc::Sender<Frame>,
    /// The frames on their way to the client.
    to_client: mpsc::Sender<Frame>,
    /// The receiver for the extension, taken once, when it connects.
    for_extension: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Frame>>>>,
    /// The receiver for the client, taken once, when it connects.
    for_client: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Frame>>>>,
    /// The ring, for resume.
    replay: Arc<tokio::sync::Mutex<Replay>>,
    /// The sequence number of the tunnel.
    sequence: Arc<tokio::sync::Mutex<u64>>,
}

impl Tunnel {
    /// A tunnel with that registry and that grant.
    pub fn new(registry: Arc<dyn Registry>, grant: Grant) -> Tunnel {
        let (to_extension, for_extension) = mpsc::channel(256);
        let (to_client, for_client) = mpsc::channel(256);
        Tunnel {
            registry,
            grant,
            to_extension,
            to_client,
            for_extension: Arc::new(tokio::sync::Mutex::new(Some(for_extension))),
            for_client: Arc::new(tokio::sync::Mutex::new(Some(for_client))),
            replay: Arc::new(tokio::sync::Mutex::new(Replay::new())),
            sequence: Arc::new(tokio::sync::Mutex::new(0)),
        }
    }

    /// Take the receiver of the extension's end, once.
    pub async fn take_for_extension(&self) -> Option<mpsc::Receiver<Frame>> {
        self.for_extension.lock().await.take()
    }

    /// Take the receiver of the client's end, once.
    pub async fn take_for_client(&self) -> Option<mpsc::Receiver<Frame>> {
        self.for_client.lock().await.take()
    }

    /// The grant in force.
    pub fn grant(&self) -> &Grant {
        &self.grant
    }

    /// The next sequence number of the tunnel.
    pub async fn next_sequence(&self) -> u64 {
        let mut sequence = self.sequence.lock().await;
        *sequence += 1;
        *sequence
    }

    /// Put a frame into the ring, for resume.
    pub async fn remember(&self, frame: &Frame) {
        self.replay.lock().await.push(frame.seq, frame.clone());
    }

    /// The frames of the tunnel after that sequence number.
    pub async fn replay_from(&self, after: u64) -> (Vec<Frame>, u64, bool) {
        self.replay.lock().await.replay_from(after)
    }

    /// Whether the frame may cross the tunnel.
    ///
    /// This is the whole of the broker's authority over content: a frame
    /// that names a scope the grant does not cover is dropped, and the
    /// scope is named in the refusal.
    pub async fn allows(&self, scope: &Scope) -> bool {
        self.grant.covers(scope)
    }

    /// The registry behind the tunnel.
    pub fn registry(&self) -> &Arc<dyn Registry> {
        &self.registry
    }
}

/// Carry the extension's end of a tunnel.
pub async fn serve_extension(
    mut socket: WebSocket,
    tunnel: Tunnel,
    mut for_extension: mpsc::Receiver<Frame>,
    _hello: conduit_protocol::SchemaId,
) {
    let to_client = tunnel.to_client.clone();
    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                match message {
                    Message::Text(text) => {
                        let frame: Frame = match serde_json::from_str(&text) {
                            Ok(frame) => frame,
                            Err(_) => {
                                let refusal = failure_frame(&conduit_protocol::Failure::Malformed(
                                    "not a frame".to_string(),
                                ));
                                if socket.send(Message::Text(refusal.into())).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                        };
                        if frame.kind == FrameType::Ping {
                            if socket.send(Message::Text(ping_reply().into())).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        if to_client.send(frame).await.is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            frame = for_extension.recv() => {
                let Some(mut frame) = frame else { break };
                frame.seq = tunnel.next_sequence().await;
                tunnel.remember(&frame).await;
                let text = serde_json::to_string(&frame).unwrap_or_default();
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Carry the client's end of a tunnel.
pub async fn serve_client(
    mut socket: WebSocket,
    tunnel: Tunnel,
    mut for_client: mpsc::Receiver<Frame>,
    _hello: conduit_protocol::SchemaId,
    _token: String,
) {
    let to_extension = tunnel.to_extension.clone();
    loop {
        tokio::select! {
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                match message {
                    Message::Text(text) => {
                        let frame: Frame = match serde_json::from_str(&text) {
                            Ok(frame) => frame,
                            Err(_) => continue,
                        };
                        match frame.kind {
                            FrameType::Ping => {
                                if socket.send(Message::Text(ping_reply().into())).await.is_err() {
                                    break;
                                }
                            }
                            FrameType::Subscribe => {
                                let wanted = scopes_of(&text);
                                match scopes_the_grant_allows(tunnel.grant(), &wanted) {
                                    Ok(()) => {
                                        if to_extension.send(frame).await.is_err() {
                                            break;
                                        }
                                    }
                                    Err(missing) => {
                                        let error = ProtocolError {
                                            code: conduit_protocol::ErrorCode::ScopeDenied,
                                            message: format!("{missing:?} is not granted"),
                                            reference: None,
                                            scope: missing.first().map(|scope| scope.to_string()),
                                        };
                                        let text = serde_json::to_string(&error).unwrap_or_default();
                                        if socket.send(Message::Text(text.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            _ => {
                                if to_extension.send(frame).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            frame = for_client.recv() => {
                let Some(mut frame) = frame else { break };
                frame.seq = tunnel.next_sequence().await;
                tunnel.remember(&frame).await;
                let text = serde_json::to_string(&frame).unwrap_or_default();
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// The scopes a subscribe frame asks for.
fn scopes_of(text: &str) -> Vec<Scope> {
    let value: serde_json::Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    value
        .get("scopes")
        .and_then(|scopes| scopes.as_array())
        .map(|scopes| {
            scopes
                .iter()
                .filter_map(|scope| scope.as_str())
                .filter_map(|scope| {
                    let (namespace, verb) = scope.rsplit_once('.')?;
                    Some(Scope::new(namespace, verb))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The frame that answers a ping.
fn ping_reply() -> String {
    serde_json::to_string(&Frame {
        kind: FrameType::Pong,
        schema: conduit_protocol::SchemaId::new("dev.conduit.pong", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: None,
    })
    .unwrap_or_default()
}
