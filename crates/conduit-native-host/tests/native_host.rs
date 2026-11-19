//! The test of the native host: the Safari near end.
//!
//! The host is a separate process that holds the socket and does the
//! sealing, so the test starts it as a process, speaks the native
//! messaging protocol to it over a Unix socket, and watches what
//! reaches the broker and what comes back.

use conduit_broker::{router, AppState, Devices, InMemoryRegistry};
use conduit_client::{Address, Client};
use conduit_protocol::{Grant, SchemaId, Scope};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UnixListener, UnixStream};

fn surface() -> SchemaId {
    SchemaId::new("dev.conduit.browser.tabs", 1)
}

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

/// Write one message in the native messaging protocol: a four-byte
/// length, then that many bytes of JSON.
async fn write_native(stream: &mut UnixStream, body: &str) {
    let bytes = body.as_bytes();
    let length = (bytes.len() as u32).to_be_bytes();
    stream.write_all(&length).await.expect("the length goes");
    stream.write_all(bytes).await.expect("the body goes");
    stream.flush().await.expect("the flush goes");
}

/// Read one message in the native messaging protocol.
async fn read_native(stream: &mut UnixStream) -> serde_json::Value {
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

/// Read until a frame of that type turns up.
async fn read_until_type(stream: &mut UnixStream, kind: &str) -> serde_json::Value {
    for _ in 0..12 {
        let frame = read_native(stream).await;
        if frame["t"] == kind {
            return frame;
        }
    }
    panic!("no frame of type {kind} turned up");
}

#[tokio::test]
async fn the_native_host_seals_and_opens_for_the_extension() {
    let (address, socket) = broker().await;

    // The listener is bound, but the task that serves it may not have
    // been polled yet: wait until the broker answers before the host
    // tries to reach it.
    for _ in 0..50 {
        if tokio::net::TcpStream::connect(socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // The native port the host will connect to.
    let path = std::env::temp_dir().join(format!(
        "conduit-native-{}-{}.sock",
        std::process::id(),
        address.len()
    ));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("the socket is bound");

    let mut host = tokio::process::Command::new(env!("CARGO_BIN_EXE_conduit-native-host"))
        .env("CONDUIT_BROKER", format!("{address}/v1/tunnel"))
        .env("CONDUIT_NATIVE_PORT", &path)
        .env("CONDUIT_SURFACE", "dev.conduit.browser.tabs/v1")
        .env("CONDUIT_SECRET", "bootstrap-secret")
        .env("CONDUIT_INSTANCE", "ext-1")
        .spawn()
        .expect("the host starts");

    // The extension's end of the native port.
    let (mut native, _) = listener.accept().await.expect("the host connects");

    // The client at the far end of the tunnel.
    //
    // The host reaches the broker on its own; the client may arrive
    // first, and the broker refuses a client whose surface has no
    // extension on it.  So the client waits for the host.
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
    let mut client = client.expect("the client connects once the host is there");
    client
        .subscribe(
            &target,
            &[Scope::new("browser.tabs", conduit_protocol::verb::READ)],
        )
        .await
        .expect("the subscription goes");

    // Give the two ends a moment to meet.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // The extension sends a frame with a plaintext payload.  The host
    // seals it, and the client opens it.
    let frame = serde_json::json!({
        "t": "publish",
        "s": "dev.conduit.browser.tab/v1",
        "c": "tabs",
        "seq": 0,
        "p": "the tab in front of the user",
    })
    .to_string();
    write_native(&mut native, &frame).await;

    let event = tokio::time::timeout(std::time::Duration::from_secs(10), client.next())
        .await
        .expect("the event arrives")
        .expect("no error")
        .expect("an event");
    assert_eq!(event.payload, "the tab in front of the user");
    assert_ne!(
        event.sealed, "the tab in front of the user",
        "the payload crossed the broker sealed"
    );

    // And the other way: what the client publishes, the extension gets
    // as it was, because the host opened it.
    client
        .publish(
            &SchemaId::new("dev.conduit.browser.tab", 1),
            "tabs",
            b"from the client",
        )
        .await
        .expect("the publish goes");

    let arrived = read_until_type(&mut native, "publish").await;
    assert_eq!(arrived["p"].as_str(), Some("from the client"));

    let _ = host.kill().await;
    let _ = std::fs::remove_file(&path);
}
