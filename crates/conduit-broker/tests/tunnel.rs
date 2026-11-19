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
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;
    registry
        .put_grant(Grant {
            surface: surface(),
            instance: "bootstrap".to_string(),
            scopes: vec![Scope::new("mail.compose", conduit_protocol::verb::READ)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
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
        devices: conduit_broker::Devices::new(),
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
        "p": serde_json::json!({
            "channel": "mail.messages",
            "scopes": ["mail.compose.write"],
        })
        .to_string(),
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
        "p": serde_json::json!({
            "channel": "mail.messages",
            "scopes": ["mail.compose.read"],
        })
        .to_string(),
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

#[tokio::test]
async fn frames_over_200_per_second_are_dropped_and_the_socket_stays_open() {
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
    read_until(&mut extension, "welcome").await;

    let (mut client, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=client&surface=dev.conduit.mail.compose%2Fv1&token=token"
    ))
    .await
    .expect("the client connects");
    client
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    read_until(&mut client, "welcome").await;

    let publish = |seq: u64| {
        serde_json::to_string(&Frame {
            kind: FrameType::Publish,
            schema: SchemaId::new("dev.conduit.mail.message", 1),
            channel: "compose".to_string(),
            call: None,
            seq,
            payload: Some("x".to_string()),
        })
        .expect("a frame is JSON")
    };
    for seq in 1..=300 {
        extension
            .send(Message::Text(publish(seq)))
            .await
            .expect("the frame goes");
    }

    let mut received = 0;
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(std::time::Duration::from_millis(500), client.next()).await
    {
        if let Message::Text(text) = message {
            if text.contains("\"publish\"") {
                received += 1;
            }
        }
    }
    assert!(
        (180..=240).contains(&received),
        "about 200 of 300 frames cross, got {received}"
    );

    // The socket is still open: after a second the bucket is full again.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    extension
        .send(Message::Text(publish(301)))
        .await
        .expect("the socket is open");
    let text = read_until(&mut client, "\"publish\"").await;
    assert!(
        text.contains("301"),
        "a frame crosses after the refill: {text}"
    );
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// An extension that has said hello and been welcomed.
async fn extension_end(address: &str) -> Socket {
    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    read_until(&mut extension, "welcome").await;
    extension
}

/// A client with that token, after its hello.
async fn client_end(address: &str, token: &str) -> Socket {
    let (mut client, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=client&surface=dev.conduit.mail.compose%2Fv1&token={token}"
    ))
    .await
    .expect("the client connects");
    client
        .send(Message::Text(hello()))
        .await
        .expect("the hello goes");
    client
}

/// Whether the socket is closed within a second.
async fn closes(socket: &mut Socket) -> bool {
    let deadline = std::time::Duration::from_secs(1);
    loop {
        match tokio::time::timeout(deadline, socket.next()).await {
            Ok(None) | Ok(Some(Err(_))) | Ok(Some(Ok(Message::Close(_)))) => return true,
            Ok(Some(Ok(_))) => continue,
            Err(_) => return false,
        }
    }
}

#[tokio::test]
async fn a_client_whose_token_does_not_cover_the_surface_is_refused() {
    let address = broker().await;
    let _extension = extension_end(&address).await;
    let mut client = client_end(&address, "not-a-token").await;
    let refusal = read_until(&mut client, "error").await;
    assert!(refusal.contains("unauthorized"), "{refusal}");
    assert!(closes(&mut client).await, "the socket is closed after");
}

#[tokio::test]
async fn a_client_with_no_extension_there_is_told_so() {
    let address = broker().await;
    let mut client = client_end(&address, "token").await;
    let refusal = read_until(&mut client, "error").await;
    assert!(refusal.contains("surface_unknown"), "{refusal}");
}

#[tokio::test]
async fn a_client_publish_without_write_in_the_grant_is_refused() {
    let address = broker().await;
    let mut extension = extension_end(&address).await;
    let mut client = client_end(&address, "token").await;
    read_until(&mut client, "welcome").await;

    let publish = serde_json::to_string(&Frame {
        kind: FrameType::Publish,
        schema: SchemaId::new("dev.conduit.mail.message", 1),
        channel: "compose".to_string(),
        call: None,
        seq: 1,
        payload: Some("c2VhbGVk".to_string()),
    })
    .expect("a frame is JSON");
    client
        .send(Message::Text(publish))
        .await
        .expect("the publish goes");
    let refusal = read_until(&mut client, "scope_denied").await;
    assert!(refusal.contains("mail.compose.write"), "{refusal}");

    // The extension saw the client's hello, and nothing after it.
    read_until(&mut extension, "hello").await;
    let more = tokio::time::timeout(std::time::Duration::from_millis(300), extension.next()).await;
    assert!(more.is_err(), "the refused publish didn't cross: {more:?}");
}

#[tokio::test]
async fn a_granted_subscription_is_answered_with_subscribed() {
    let address = broker().await;
    let _extension = extension_end(&address).await;
    let mut client = client_end(&address, "token").await;
    read_until(&mut client, "welcome").await;
    let subscribe = serde_json::json!({
        "t": "subscribe",
        "s": "dev.conduit.subscribe/v1",
        "c": "compose",
        "seq": 1,
        "p": serde_json::json!({ "channel": "compose", "scopes": ["mail.compose.read"] }).to_string(),
    })
    .to_string();
    client
        .send(Message::Text(subscribe))
        .await
        .expect("the subscription goes");
    let answer = read_until(&mut client, "subscribed").await;
    assert!(answer.contains("mail.compose.read"), "{answer}");
}

#[tokio::test]
async fn when_the_extension_leaves_its_client_is_closed() {
    let address = broker().await;
    let mut extension = extension_end(&address).await;
    let mut client = client_end(&address, "token").await;
    read_until(&mut client, "welcome").await;
    extension.close(None).await.expect("the extension leaves");
    assert!(closes(&mut client).await, "the client is closed with it");
}

#[tokio::test]
async fn a_second_client_attaches_after_the_first_leaves_and_gets_the_extension_hello() {
    let address = broker().await;
    let mut extension = extension_end(&address).await;

    let mut first = client_end(&address, "token").await;
    read_until(&mut first, "welcome").await;
    read_until(&mut first, "hello").await;

    // While the first holds the tunnel, a second is turned away.
    let mut second = client_end(&address, "token").await;
    let refusal = read_until(&mut second, "error").await;
    assert!(refusal.contains("another client"), "{refusal}");

    first.close(None).await.expect("the first leaves");
    let _ = closes(&mut first).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut third = client_end(&address, "token").await;
    read_until(&mut third, "welcome").await;
    read_until(&mut third, "hello").await;

    // And the extension heard the new client's hello.
    read_until(&mut extension, "hello").await;
    read_until(&mut extension, "hello").await;
}
