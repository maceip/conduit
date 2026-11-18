//! The tests that put two ends through the broker and watch a frame cross.
//!
//! These are real sockets over a real server: the extension and the
//! client are separate WebSocket connections, and the frames travel
//! through the broker's scope check on the way.

use conduit_broker::{router, AppState, InMemoryRegistry};
use conduit_protocol::{Frame, FrameType, Grant, SchemaId, Scope};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

/// The surface of the tests.
fn surface() -> SchemaId {
    SchemaId::new("dev.conduit.mail.compose", 1)
}

/// A registry with one surface, one grant and one token.
async fn registry() -> Arc<InMemoryRegistry> {
    let registry = InMemoryRegistry::new();
    registry
        .put_surface(conduit_protocol::Surface {
            id: surface(),
            schemas: vec![SchemaId::new("dev.conduit.mail.message", 1)],
            scopes: vec![
                Scope::new("mail.compose", conduit_protocol::verb::READ),
                Scope::new("mail.compose", conduit_protocol::verb::WRITE),
            ],
            schemes: vec!["x25519-chacha20poly1305/v1".to_string()],
        })
        .await;
    registry
        .put_grant(Grant {
            surface: surface(),
            instance: "bootstrap".to_string(),
            scopes: vec![Scope::new("mail.compose", conduit_protocol::verb::READ)],
            schemes: vec!["x25519-chacha20poly1305/v1".to_string()],
        })
        .await;
    registry.put_token("token", vec![surface()]).await;
    registry
}

/// Start the broker on a free port, and give its address.
async fn broker() -> String {
    let registry = registry().await;
    let state = AppState {
        registry,
        tunnels: Arc::new(tokio::sync::Mutex::new(Default::default())),
        extension_secret: "secret".to_string(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the address");
    tokio::spawn(async move {
        axum::serve(listener, router(state))
            .await
            .expect("the server runs");
    });
    format!("ws://{address}")
}

/// The hello of an end.
fn hello() -> String {
    serde_json::to_string(&Frame {
        kind: FrameType::Hello,
        schema: SchemaId::new("dev.conduit.hello", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: None,
    })
    .expect("a frame is JSON")
}

/// Read text frames until one of them holds the needle.
///
/// The broker hands the hello of one end to the other end, so a reader
/// that wants one particular frame may see the hello first.
async fn read_until(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    needle: &str,
) -> String {
    for _ in 0..8 {
        let text = read(socket).await;
        if text.contains(needle) {
            return text;
        }
    }
    panic!("no frame held {needle}");
}

/// Read one text frame.
async fn read(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> String {
    loop {
        let message = socket
            .next()
            .await
            .expect("a message")
            .expect("a good message");
        if let Message::Text(text) = message {
            return text.to_string();
        }
    }
}

#[tokio::test]
async fn a_frame_from_the_extension_crosses_to_the_client() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let welcome = read_until(&mut extension, "welcome").await;
    assert!(
        welcome.contains("welcome"),
        "the broker answers the hello with a welcome: {welcome}"
    );

    let (mut client, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=client&surface=dev.conduit.mail.compose%2Fv1&token=token"
    ))
    .await
    .expect("the client connects");
    client
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let welcome = read(&mut client).await;
    assert!(
        welcome.contains("welcome"),
        "the client is welcomed: {welcome}"
    );

    let publish = serde_json::to_string(&Frame {
        kind: FrameType::Publish,
        schema: SchemaId::new("dev.conduit.mail.message", 1),
        channel: "mail.messages".to_string(),
        call: None,
        seq: 7,
        payload: Some("c2VhbGVk".to_string()),
    })
    .expect("a frame is JSON");
    extension
        .send(Message::Text(publish))
        .await
        .expect("the publish goes");

    let crossed = read_until(&mut client, "publish").await;
    let frame: Frame = serde_json::from_str(&crossed).expect("the frame is JSON");
    assert_eq!(frame.kind, FrameType::Publish);
    assert_eq!(frame.channel, "mail.messages");
    assert_eq!(frame.payload.as_deref(), Some("c2VhbGVk"));
    assert_eq!(
        frame.seq, 7,
        "the broker keeps the sequence number of the end that sent the frame"
    );
}

#[tokio::test]
async fn a_subscription_for_a_scope_that_was_not_granted_is_refused() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let _ = read_until(&mut extension, "welcome").await;

    let (mut client, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=client&surface=dev.conduit.mail.compose%2Fv1&token=token"
    ))
    .await
    .expect("the client connects");
    client
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let _ = read_until(&mut client, "welcome").await;

    // The grant covers read, not write.
    let subscribe = serde_json::json!({
        "t": "subscribe",
        "s": "dev.conduit.subscribe/v1",
        "c": "mail.messages",
        "seq": 0,
        "scopes": ["mail.compose.write"],
    })
    .to_string();
    client
        .send(Message::Text(subscribe))
        .await
        .expect("the subscription goes");

    let refusal = read_until(&mut client, "scope_denied").await;
    assert!(
        refusal.contains("scope_denied"),
        "the scope that was not granted is named: {refusal}"
    );
    assert!(
        refusal.contains("mail.compose.write"),
        "the refusal names the scope: {refusal}"
    );
}

#[tokio::test]
async fn a_subscription_for_a_scope_that_was_granted_goes_through() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let _ = read_until(&mut extension, "welcome").await;

    let (mut client, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=client&surface=dev.conduit.mail.compose%2Fv1&token=token"
    ))
    .await
    .expect("the client connects");
    client
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    let _ = read_until(&mut client, "welcome").await;

    let subscribe = serde_json::json!({
        "t": "subscribe",
        "s": "dev.conduit.subscribe/v1",
        "c": "mail.messages",
        "seq": 0,
        "scopes": ["mail.compose.read"],
    })
    .to_string();
    client
        .send(Message::Text(subscribe))
        .await
        .expect("the subscription goes");

    // The subscription reaches the extension, which is the other end.
    let arrived = read_until(&mut extension, "subscribe").await;
    assert!(
        arrived.contains("subscribe"),
        "the subscription reaches the extension: {arrived}"
    );
}

#[tokio::test]
async fn an_extension_with_the_wrong_secret_is_refused() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=wrong"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");

    let refusal = read_until(&mut extension, "unauthorized").await;
    assert!(
        refusal.contains("unauthorized"),
        "the wrong secret is refused: {refusal}"
    );
}
