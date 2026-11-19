//! The test of the near end: the process that holds the socket of a
//! surface.
//!
//! The test starts a broker, starts the near end as a process with a
//! configuration, and plays the part of the plugin inside the app: it
//! opens the near end's socket, publishes what the app knows, and
//! watches what comes back.

use conduit_broker::{router, AppState, Devices, InMemoryRegistry};
use conduit_client::{Address, Client};
use conduit_protocol::{Grant, SchemaId, Scope};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixStream};

/// The surface of this test.
fn surface() -> SchemaId {
    SchemaId::new("dev.conduit.browser.tabs", 1)
}

/// A broker with a grant for the surface.
async fn broker() -> (String, std::net::SocketAddr) {
    let registry = InMemoryRegistry::new();
    registry
        .put_grant(Grant {
            surface: surface(),
            instance: "ext-1".to_string(),
            scopes: vec![
                Scope::new("browser.tabs", conduit_protocol::verb::READ),
                Scope::new("browser.tabs", conduit_protocol::verb::WRITE),
            ],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;
    registry.put_token("token", vec![surface()]).await;
    let state = AppState {
        registry,
        tunnels: Arc::new(tokio::sync::Mutex::new(Default::default())),
        devices: Devices::new(),
        extension_secret: "bootstrap-secret".to_string(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the address");
    tokio::spawn(async move {
        axum::serve(listener, router(state))
            .await
            .expect("the server runs");
    });
    (format!("ws://{address}"), address)
}

/// Write one message to the near end: a four-byte big-endian length,
/// then that many bytes of JSON.
async fn write_message(stream: &mut UnixStream, body: &str) {
    let bytes = body.as_bytes();
    let length = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&length).await.expect("the length goes");
    stream.write_all(bytes).await.expect("the body goes");
    stream.flush().await.expect("the flush goes");
}

/// Read one message from the near end.
async fn read_message(stream: &mut UnixStream) -> serde_json::Value {
    let mut length = [0u8; 4];
    stream
        .read_exact(&mut length)
        .await
        .expect("the length comes");
    let length = u32::from_be_bytes(length) as usize;
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).await.expect("the body comes");
    serde_json::from_slice(&body).expect("the body is JSON")
}

/// Read until a message with that op turns up.
async fn read_until_op(stream: &mut UnixStream, op: &str) -> serde_json::Value {
    for _ in 0..32 {
        let message = read_message(stream).await;
        if message["op"] == op {
            return message;
        }
    }
    panic!("no message with the op {op} turned up");
}

#[tokio::test]
async fn the_near_end_carries_a_surface_from_the_plugin_to_the_client() {
    let (address, socket) = broker().await;

    for _ in 0..50 {
        if tokio::net::TcpStream::connect(socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // The socket of the near end, and the configuration that names it.
    let directory = std::env::temp_dir().join(format!(
        "conduit-near-end-{}-{}",
        std::process::id(),
        socket.port()
    ));
    std::fs::create_dir_all(&directory).expect("the directory is made");
    let path = directory.join("tabs.sock");
    let configuration = directory.join("near-end.json");
    std::fs::write(
        &configuration,
        serde_json::json!({
            "broker": format!("{address}/v1/tunnel"),
            "instance": "ext-1",
            "surfaces": [{
                "id": "dev.conduit.browser.tabs/v1",
                "channel": "tabs",
                "schemas": ["dev.conduit.browser.tab/v1"],
                "scopes": ["browser.tabs.read"],
                "socket": path.to_string_lossy(),
                "secret": "bootstrap-secret",
            }],
        })
        .to_string(),
    )
    .expect("the configuration is written");

    let mut near_end = tokio::process::Command::new(env!("CARGO_BIN_EXE_conduit-near-end"))
        .arg(&configuration)
        .spawn()
        .expect("the near end starts");

    // The plugin waits for the socket to turn up.
    let mut plugin = None;
    for _ in 0..50 {
        if let Ok(stream) = UnixStream::connect(&path).await {
            plugin = Some(stream);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let mut plugin = plugin.expect("the near end binds its socket");

    // The client at the far end of the tunnel.
    let target =
        Address::parse("conduit://dev.conduit.browser.tabs/tabs@v1").expect("the address parses");
    let mut client = None;
    for _ in 0..50 {
        match Client::connect(&format!("{address}/v1/tunnel"), "token", &target).await {
            Ok(connected) => {
                client = Some(connected);
                break;
            }
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
        }
    }
    let mut client = client.expect("the client connects once the near end is there");
    client
        .subscribe(
            &target,
            &[Scope::new("browser.tabs", conduit_protocol::verb::READ)],
        )
        .await
        .expect("the subscription goes");

    // Wait until the two ends have agreed their keys.  The near end
    // says so in its status, and the near end cannot seal anything
    // before that: a guess with a sleep is a guess.
    write_message(&mut plugin, r#"{"op":"status"}"#).await;
    loop {
        let message = read_message(&mut plugin).await;
        if message["op"] == "status" && message["agreed"] == true {
            break;
        }
    }

    // The plugin publishes what the app knows.  The near end seals it.
    write_message(
        &mut plugin,
        &serde_json::json!({
            "op": "publish",
            "schema": "dev.conduit.browser.tab/v1",
            "payload": {"url": "https://example.test/", "title": "Example"},
        })
        .to_string(),
    )
    .await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(10), client.next())
        .await
        .expect("the event arrives")
        .expect("no error")
        .expect("an event");
    let payload: serde_json::Value =
        serde_json::from_str(&event.payload).expect("the payload is JSON");
    assert_eq!(payload["url"], "https://example.test/");
    assert_eq!(payload["title"], "Example");
    assert_ne!(
        event.sealed, event.payload,
        "the payload crossed the broker sealed"
    );

    // And the other way: what the client publishes, the plugin gets as
    // it was, because the near end opened it.
    client
        .publish(
            &SchemaId::new("dev.conduit.browser.tab", 1),
            "tabs",
            b"from the client",
        )
        .await
        .expect("the publish goes");

    let event = read_until_op(&mut plugin, "event").await;
    assert_eq!(event["payload"], "from the client");
    assert_eq!(event["schema"], "dev.conduit.browser.tab/v1");

    let _ = near_end.kill().await;
    let _ = std::fs::remove_dir_all(&directory);
}
