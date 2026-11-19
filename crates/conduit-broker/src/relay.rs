//! The relay: the frames between the two ends.
//!
//! One end sends, the other receives, and the broker is in the middle
//! with the grant in its hand.  The broker reads the envelope of every
//! frame, checks the scope it names against the grant, and forwards the
//! frame *sealed* — it never opens the payload.
//!
//! The frames keep the sequence number of the end that sent them.  That
//! number is part of the nonce the payload is sealed under, so a frame
//! that the broker renumbered could not be opened.  The broker numbers
//! nothing of its own.

use crate::{error_frame, failure_frame, registry::Registry, scopes_the_grant_allows};
use axum::extract::ws::{Message, WebSocket};
use conduit_protocol::{verb, ErrorCode, Frame, FrameType, Grant, Replay, Scope};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{mpsc, watch};

/// Frames per second a tunnel may carry, both ends together
/// (docs/protocol.md, size and rate).
pub const FRAMES_PER_SECOND: u32 = 200;

/// A token bucket: holds up to `rate` frames and refills at `rate` per
/// second. A frame over the limit is dropped, not answered, and the
/// socket stays open.
#[derive(Debug)]
pub struct RateLimit {
    rate: f64,
    tokens: f64,
    last: Instant,
}

impl RateLimit {
    /// A full bucket of `rate` frames per second, starting at `now`.
    pub fn new(rate: u32, now: Instant) -> RateLimit {
        RateLimit {
            rate: f64::from(rate),
            tokens: f64::from(rate),
            last: now,
        }
    }

    /// Whether one more frame may pass at `now`.
    pub fn admit(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.rate);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// A tunnel: one extension, and the client attached to it.
///
/// The tunnel owns the two channels that connect the ends: the frames
/// going to the extension, and the frames going to the client.  One
/// client at a time holds the client's end; when it leaves, the end is
/// handed back so the next client can attach.
#[derive(Clone)]
pub struct Tunnel {
    /// The registry the tunnel asks about surfaces and grants.
    registry: Arc<dyn Registry>,
    /// The grant in force.
    grant: Grant,
    /// The frames on their way to the extension.
    pub(crate) to_extension: mpsc::Sender<Frame>,
    /// The frames on their way to the client.
    pub(crate) to_client: mpsc::Sender<Frame>,
    /// The receiver for the extension, taken once, when it connects.
    for_extension: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Frame>>>>,
    /// The receiver for the client, held by the client attached now.
    for_client: Arc<tokio::sync::Mutex<Option<mpsc::Receiver<Frame>>>>,
    /// The hello of the extension: every client that attaches needs its
    /// public key.
    extension_hello: Arc<tokio::sync::Mutex<Option<Frame>>>,
    /// The frames the extension sent, for resume.
    from_extension: Arc<tokio::sync::Mutex<Replay>>,
    /// The frames the client sent, for resume.
    from_client: Arc<tokio::sync::Mutex<Replay>>,
    /// The rate limit, shared by both ends of the tunnel.
    limit: Arc<std::sync::Mutex<RateLimit>>,
    /// How many frames the rate limit dropped.
    dropped: Arc<AtomicU64>,
    /// Set when the extension's end is gone => the client's end closes too.
    closed: Arc<watch::Sender<bool>>,
}

impl Tunnel {
    /// A tunnel with that registry and that grant.
    pub fn new(registry: Arc<dyn Registry>, grant: Grant) -> Tunnel {
        let (to_extension, for_extension) = mpsc::channel(256);
        let (to_client, for_client) = mpsc::channel(256);
        let (closed, _) = watch::channel(false);
        Tunnel {
            registry,
            grant,
            to_extension,
            to_client,
            for_extension: Arc::new(tokio::sync::Mutex::new(Some(for_extension))),
            for_client: Arc::new(tokio::sync::Mutex::new(Some(for_client))),
            extension_hello: Arc::new(tokio::sync::Mutex::new(None)),
            from_extension: Arc::new(tokio::sync::Mutex::new(Replay::new())),
            from_client: Arc::new(tokio::sync::Mutex::new(Replay::new())),
            limit: Arc::new(std::sync::Mutex::new(RateLimit::new(
                FRAMES_PER_SECOND,
                Instant::now(),
            ))),
            dropped: Arc::new(AtomicU64::new(0)),
            closed: Arc::new(closed),
        }
    }

    /// Whether one more frame may cross now; counts the ones that may not.
    pub fn admit(&self) -> bool {
        let admitted = self
            .limit
            .lock()
            .map(|mut limit| limit.admit(Instant::now()))
            .unwrap_or(false);
        if !admitted {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        admitted
    }

    /// How many frames the rate limit has dropped so far.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Take the receiver of the extension's end, once.
    pub async fn take_for_extension(&self) -> Option<mpsc::Receiver<Frame>> {
        self.for_extension.lock().await.take()
    }

    /// Take the receiver of the client's end; `None` while another
    /// client holds it.
    ///
    /// Frames that waited for no one are dropped: they were sealed for a
    /// client that has left, under keys the next one doesn't have.
    pub async fn take_for_client(&self) -> Option<mpsc::Receiver<Frame>> {
        let mut receiver = self.for_client.lock().await.take()?;
        while receiver.try_recv().is_ok() {}
        Some(receiver)
    }

    /// Hand the client's end back, when the client leaves.
    pub async fn give_back_for_client(&self, receiver: mpsc::Receiver<Frame>) {
        *self.for_client.lock().await = Some(receiver);
    }

    /// Keep the extension's hello, for the clients that attach later.
    pub async fn set_extension_hello(&self, hello: Frame) {
        *self.extension_hello.lock().await = Some(hello);
    }

    /// The extension's hello, if it has said it.
    pub async fn extension_hello(&self) -> Option<Frame> {
        self.extension_hello.lock().await.clone()
    }

    /// Close the tunnel: both ends stop.
    pub fn close(&self) {
        self.closed.send_replace(true);
    }

    /// Whether the tunnel is closed.
    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// A watch on the tunnel being closed.
    pub fn closing(&self) -> watch::Receiver<bool> {
        self.closed.subscribe()
    }

    /// Whether two handles are the same tunnel.
    pub fn same(&self, other: &Tunnel) -> bool {
        Arc::ptr_eq(&self.closed, &other.closed)
    }

    /// The grant in force.
    pub fn grant(&self) -> &Grant {
        &self.grant
    }

    /// The scope a frame that exercises `verb` needs, if the grant has no
    /// scope with that verb.
    pub fn denies(&self, verb: &str) -> Option<Scope> {
        if self.grant.scopes.iter().any(|scope| scope.is(verb)) {
            return None;
        }
        let namespace = self
            .grant
            .scopes
            .first()
            .map(|scope| scope.namespace().to_string())
            .unwrap_or_else(|| self.grant.surface.name().to_string());
        Some(Scope::new(&namespace, verb))
    }

    /// Put a frame into the ring of the extension's frames, for resume.
    pub async fn remember_from_extension(&self, frame: &Frame) {
        self.from_extension
            .lock()
            .await
            .push(frame.seq, frame.clone());
    }

    /// Put a frame into the ring of the client's frames, for resume.
    pub async fn remember_from_client(&self, frame: &Frame) {
        self.from_client.lock().await.push(frame.seq, frame.clone());
    }

    /// The frames the extension sent, after that sequence number.
    pub async fn replay_from_extension(&self, after: u64) -> (Vec<Frame>, u64, bool) {
        self.from_extension.lock().await.replay_from(after)
    }

    /// The frames the client sent, after that sequence number.
    pub async fn replay_from_client(&self, after: u64) -> (Vec<Frame>, u64, bool) {
        self.from_client.lock().await.replay_from(after)
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

/// The refusal of a scope, as an error frame.
fn scope_denied(scope: &Scope, what: &str) -> String {
    error_frame(
        ErrorCode::ScopeDenied,
        &format!("not granted: {scope} => {what}"),
        Some(scope.to_string()),
    )
}

/// Carry the extension's end of a tunnel.
pub async fn serve_extension(
    mut socket: WebSocket,
    tunnel: Tunnel,
    mut for_extension: mpsc::Receiver<Frame>,
) {
    let mut closing = tunnel.closing();
    if tunnel.is_closed() {
        return;
    }
    loop {
        tokio::select! {
            _ = closing.changed() => break,
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                match message {
                    Message::Text(text) => {
                        if !tunnel.admit() {
                            continue;
                        }
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
                        match frame.kind {
                            FrameType::Ping => {
                                if socket.send(Message::Text(ping_reply().into())).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            FrameType::Publish => {
                                if let Some(scope) = tunnel.denies(verb::READ) {
                                    let refusal = scope_denied(&scope, "the extension may not publish");
                                    if socket.send(Message::Text(refusal.into())).await.is_err() {
                                        break;
                                    }
                                    continue;
                                }
                            }
                            _ => {}
                        }
                        tunnel.remember_from_extension(&frame).await;
                        // No client attached and the buffer full => the
                        // frame is dropped; it would be sealed for nobody.
                        let _ = tunnel.to_client.try_send(frame);
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            frame = for_extension.recv() => {
                let Some(frame) = frame else { break };
                let text = serde_json::to_string(&frame).unwrap_or_default();
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    tunnel.close();
    let _ = socket.send(Message::Close(None)).await;
}

/// Carry the client's end of a tunnel; hands the client's receiver
/// back when the client leaves.
pub async fn serve_client(
    mut socket: WebSocket,
    tunnel: Tunnel,
    mut for_client: mpsc::Receiver<Frame>,
) -> mpsc::Receiver<Frame> {
    let mut closing = tunnel.closing();
    if tunnel.is_closed() {
        let _ = socket.send(Message::Close(None)).await;
        return for_client;
    }
    loop {
        tokio::select! {
            _ = closing.changed() => break,
            message = socket.recv() => {
                let Some(Ok(message)) = message else { break };
                match message {
                    Message::Text(text) => {
                        if !tunnel.admit() {
                            continue;
                        }
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
                        let refusal = match frame.kind {
                            FrameType::Ping => {
                                if socket.send(Message::Text(ping_reply().into())).await.is_err() {
                                    break;
                                }
                                continue;
                            }
                            FrameType::Subscribe => {
                                let wanted = scopes_of(&frame);
                                match scopes_the_grant_allows(tunnel.grant(), &wanted) {
                                    Ok(()) => {
                                        let reply = subscribed(&frame, &wanted);
                                        let _ = tunnel.to_extension.try_send(frame);
                                        if socket.send(Message::Text(reply.into())).await.is_err() {
                                            break;
                                        }
                                        continue;
                                    }
                                    Err(missing) => Some(error_frame(
                                        ErrorCode::ScopeDenied,
                                        &format!(
                                            "not granted: {}",
                                            missing
                                                .iter()
                                                .map(|scope| scope.to_string())
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        ),
                                        missing.first().map(|scope| scope.to_string()),
                                    )),
                                }
                            }
                            FrameType::Publish => tunnel
                                .denies(verb::WRITE)
                                .map(|scope| scope_denied(&scope, "the client may not publish")),
                            FrameType::Call => tunnel
                                .denies(verb::CALL)
                                .map(|scope| scope_denied(&scope, "the client may not call")),
                            // The connect frame belongs to the hop
                            // between a plugin and the near end.  It
                            // never goes near the tunnel, and the
                            // broker says so.
                            FrameType::Connect => Some(error_frame(
                                ErrorCode::UnknownType,
                                "the connect frame does not go on the tunnel",
                                None,
                            )),
                            _ => None,
                        };
                        if let Some(refusal) = refusal {
                            if socket.send(Message::Text(refusal.into())).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        tunnel.remember_from_client(&frame).await;
                        let _ = tunnel.to_extension.try_send(frame);
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            frame = for_client.recv() => {
                let Some(frame) = frame else { break };
                let text = serde_json::to_string(&frame).unwrap_or_default();
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = socket.send(Message::Close(None)).await;
    for_client
}

/// The answer to a subscription the grant covers.
fn subscribed(frame: &Frame, scopes: &[Scope]) -> String {
    serde_json::to_string(&Frame {
        kind: FrameType::Subscribed,
        schema: conduit_protocol::SchemaId::new("dev.conduit.subscribed", 1),
        channel: frame.channel.clone(),
        call: None,
        seq: 0,
        payload: Some(serde_json::json!({
            "channel": frame.channel,
            "scopes": scopes.iter().map(|scope| scope.to_string()).collect::<Vec<_>>(),
        })),
    })
    .unwrap_or_default()
}

/// The scopes a subscribe frame asks for, from its payload
/// (`dev.conduit.subscribe/v1`; subscribe is not sealed, the broker has
/// to read it).
fn scopes_of(frame: &Frame) -> Vec<Scope> {
    let Some(payload) = frame.payload.as_ref().and_then(|body| body.as_str()) else {
        return Vec::new();
    };
    let value: serde_json::Value = match serde_json::from_str(payload) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_bucket_lets_200_through_then_drops() {
        let start = Instant::now();
        let mut limit = RateLimit::new(FRAMES_PER_SECOND, start);
        let passed = (0..300).filter(|_| limit.admit(start)).count();
        assert_eq!(passed, 200);
    }

    #[test]
    fn the_bucket_refills_at_the_rate() {
        let start = Instant::now();
        let mut limit = RateLimit::new(FRAMES_PER_SECOND, start);
        for _ in 0..200 {
            assert!(limit.admit(start));
        }
        assert!(!limit.admit(start));
        let later = start + Duration::from_millis(100);
        let passed = (0..100).filter(|_| limit.admit(later)).count();
        assert_eq!(passed, 20, "100 ms at 200/s refills 20 frames");
        let much_later = later + Duration::from_secs(10);
        let passed = (0..500).filter(|_| limit.admit(much_later)).count();
        assert_eq!(passed, 200, "the bucket never holds more than 1s of frames");
    }
}
