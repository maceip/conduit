//! The test that puts the whole tunnel together: the extension, the
//! broker, and the client library, over real sockets.

use conduit_broker::{router, AppState, InMemoryRegistry};
use conduit_client::{Address, Client};
use conduit_protocol::{Frame, FrameType, Grant, SchemaId, Scope};
use futures_util::SinkExt;
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
            schemes: vec!["x25519-chacha20poly1305/v1".to_string()],
        })
        .await;
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

#[tokio::test]
async fn the_client_receives_what_the_extension_publishes() {
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

    let target = Address::parse("conduit://dev.conduit.mail.compose/compose@v1")
        .expect("the address parses");
    let mut client = Client::connect(&format!("{address}/v1/tunnel"), "token", &target)
        .await
        .expect("the client connects");
    let _subscription = client
        .subscribe(
            &target,
            &[Scope::new("mail.compose", conduit_protocol::verb::READ)],
        )
        .await
        .expect("the subscription goes");

    // Give the subscription a moment to reach the broker.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let publish = serde_json::to_string(&Frame {
        kind: FrameType::Publish,
        schema: SchemaId::new("dev.conduit.mail.message", 1),
        channel: "compose".to_string(),
        call: None,
        seq: 0,
        payload: Some("c2VhbGVk".to_string()),
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
    assert_eq!(event.payload, "c2VhbGVk");
    assert!(event.seq > 0, "the broker numbered the event");
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
