//! The test that puts the whole tunnel together: the extension, the
//! broker, and the client library, over real sockets, with the payload
//! sealed end to end.

use conduit_broker::{router, AppState, InMemoryRegistry};
use conduit_client::{Address, Client};
use conduit_protocol::sealed::{self, KeyPair};
use conduit_protocol::{Frame, FrameType, Grant, SchemaId, Scope};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

fn surface() -> SchemaId {
    SchemaId::new("dev.conduit.mail.compose", 1)
}

async fn broker() -> String {
    let registry = InMemoryRegistry::new();
    registry
        .put_grant(Grant {
            surface: surface(),
            instance: "bootstrap".to_string(),
            scopes: vec![Scope::new("mail.compose", conduit_protocol::verb::READ)],
            schemes: vec![sealed::SCHEME.to_string()],
        })
        .await;
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

/// The hello of an end: who it is, and its public key.
///
/// The payload is what the two ends agree the key of the tunnel on.
fn hello(role: &str, keys: &KeyPair, prefix: [u8; 4]) -> String {
    let payload = serde_json::json!({
        "role": role,
        "surface": surface().to_string(),
        "crypto": {
            "scheme": sealed::SCHEME,
            "public": sealed::public_key_hex(&keys.public_bytes()),
            "prefix": sealed::public_key_hex(&prefix),
        },
    })
    .to_string();
    serde_json::to_string(&Frame {
        kind: FrameType::Hello,
        schema: SchemaId::new("dev.conduit.hello", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: Some(payload),
    })
    .expect("a frame is JSON")
}

/// The public key the peer sent, out of a hello frame.
fn public_of(frame: &str) -> [u8; 65] {
    let value: serde_json::Value = serde_json::from_str(frame).expect("a frame is JSON");
    let public = value["p"].as_str().expect("a payload");
    let inner: serde_json::Value = serde_json::from_str(public).expect("the payload is JSON");
    let hex = inner["crypto"]["public"].as_str().expect("a public key");
    let mut bytes = [0u8; 65];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex");
    }
    bytes
}

/// The nonce prefix the peer sent, out of a hello frame.
fn prefix_of(frame: &str) -> [u8; 4] {
    let value: serde_json::Value = serde_json::from_str(frame).expect("a frame is JSON");
    let public = value["p"].as_str().expect("a payload");
    let inner: serde_json::Value = serde_json::from_str(public).expect("the payload is JSON");
    let hex = inner["crypto"]["prefix"].as_str().expect("a prefix");
    let mut bytes = [0u8; 4];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).expect("hex");
    }
    bytes
}

#[tokio::test]
async fn the_client_receives_what_the_extension_publishes() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    let keys = KeyPair::generate();
    let prefix = sealed::random_prefix();
    extension
        .send(Message::Text(hello("extension", &keys, prefix)))
        .await
        .expect("the hello goes");

    let target = Address::parse("conduit://dev.conduit.mail.compose/compose@v1")
        .expect("the address parses");
    let mut client = Client::connect(&format!("{address}/v1/tunnel"), "token", &target)
        .await
        .expect("the client connects");

    client
        .subscribe(
            &target,
            &[Scope::new("mail.compose", conduit_protocol::verb::READ)],
        )
        .await
        .expect("the subscription goes");

    // The broker hands the hello of one end to the other.
    let mut their_hello = String::new();
    while !their_hello.contains("\"hello\"") {
        let message = extension
            .next()
            .await
            .expect("a message")
            .expect("a message");
        if let Message::Text(text) = message {
            eprintln!("TEST extension saw: {}", text);
            their_hello = text.to_string();
        }
    }
    let their_public = public_of(&their_hello);
    let shared = keys.agree(&their_public);

    // The extension seals the payload under the key of the tunnel; the
    // broker carries it without opening it.
    let sealed = sealed::seal(&shared.to_extension(), prefix, 1, b"sealed");
    let publish = serde_json::to_string(&Frame {
        kind: FrameType::Publish,
        schema: SchemaId::new("dev.conduit.mail.message", 1),
        channel: "compose".to_string(),
        call: None,
        seq: 1,
        payload: Some(sealed.to_base64()),
    })
    .expect("a frame is JSON");
    extension
        .send(Message::Text(publish))
        .await
        .expect("the publish goes");

    let event = tokio::time::timeout(std::time::Duration::from_secs(5), client.next())
        .await
        .expect("the event arrives")
        .expect("no error")
        .expect("an event");
    assert_eq!(event.schema.to_string(), "dev.conduit.mail.message/v1");
    assert_eq!(event.channel, "compose");
    assert_eq!(event.payload, "sealed");
    assert_ne!(event.sealed, "sealed", "the payload arrived sealed");
    assert!(event.seq > 0, "the broker numbered the event");
}

#[tokio::test]
async fn what_the_client_publishes_the_extension_opens() {
    let address = broker().await;

    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{address}/v1/tunnel?role=extension&surface=dev.conduit.mail.compose%2Fv1&secret=secret"
    ))
    .await
    .expect("the extension connects");
    let keys = KeyPair::generate();
    let prefix = sealed::random_prefix();
    extension
        .send(Message::Text(hello("extension", &keys, prefix)))
        .await
        .expect("the hello goes");

    let target = Address::parse("conduit://dev.conduit.mail.compose/compose@v1")
        .expect("the address parses");
    let mut client = Client::connect(&format!("{address}/v1/tunnel"), "token", &target)
        .await
        .expect("the client connects");

    // Give the two ends a moment to meet.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    client
        .publish(
            &SchemaId::new("dev.conduit.mail.message", 1),
            "compose",
            b"from the client",
        )
        .await
        .expect("the publish goes");

    let mut their_hello = String::new();
    while !their_hello.contains("\"hello\"") {
        let message = extension
            .next()
            .await
            .expect("a message")
            .expect("a message");
        if let Message::Text(text) = message {
            eprintln!("TEST extension saw: {}", text);
            their_hello = text.to_string();
        }
    }
    let their_public = public_of(&their_hello);
    let their_prefix = prefix_of(&their_hello);
    let shared = keys.agree(&their_public);

    // The frame that arrives is sealed under the key of the tunnel.
    let mut publish = String::new();
    while !publish.contains("\"publish\"") {
        let message = extension
            .next()
            .await
            .expect("a message")
            .expect("a message");
        if let Message::Text(text) = message {
            publish = text.to_string();
        }
    }
    let value: serde_json::Value = serde_json::from_str(&publish).expect("a frame is JSON");
    let blob = value["p"].as_str().expect("the payload");
    let sealed = sealed::Sealed::from_base64(blob).expect("the blob");
    let opened = sealed::open(
        &shared.to_client(),
        their_prefix,
        value["seq"].as_u64().unwrap(),
        &sealed,
    );
    assert_eq!(
        opened.as_deref(),
        Some(&b"from the client"[..]),
        "the extension opens what the client sealed"
    );
}

#[test]
fn an_address_names_a_surface_and_a_channel() {
    let address = Address::parse("conduit://mail/compose@v1").expect("the address parses");
    assert_eq!(address.scheme, "conduit");
    assert_eq!(address.authority, "mail");
    assert_eq!(address.path, "/compose");
    assert_eq!(address.version, 1);
    assert_eq!(address.surface().to_string(), "mail/v1");
    assert_eq!(address.channel(), "compose");
}

#[test]
fn an_address_without_a_scheme_is_not_an_address() {
    assert!(Address::parse("mail/compose@v1").is_err());
    assert!(Address::parse("conduit:///compose@v1").is_err());
    assert!(Address::parse("conduit://mail/compose@x").is_err());
}
