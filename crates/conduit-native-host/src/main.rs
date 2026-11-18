//! The native host: the Safari near end.
//!
//! Safari's background page is unloaded after about thirty seconds of
//! silence, and its WebSocket to 127.0.0.1 is broken.  So on Safari the
//! socket lives in a native process, and the extension reaches it over
//! native messaging.
//!
//! The protocol of native messaging is simple: a four-byte big-endian
//! length, then that many bytes of JSON, on both sides.
//!
//! The host does three things and nothing else: it holds the socket to
//! the broker, it moves frames between the socket and the native port,
//! and it keeps the socket alive with a ping.

use conduit_protocol::{Frame, FrameType, SchemaId};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// The configuration of the host, from the environment.
struct Settings {
    broker: String,
    surface: String,
    secret: String,
}

impl Settings {
    /// The settings, from the environment the browser sets.
    fn from_the_environment() -> Settings {
        Settings {
            broker: std::env::var("CONDUIT_BROKER")
                .unwrap_or_else(|_| "wss://broker.example/v1/tunnel".to_string()),
            surface: std::env::var("CONDUIT_SURFACE")
                .unwrap_or_else(|_| "dev.conduit.mail.compose/v1".to_string()),
            secret: std::env::var("CONDUIT_SECRET").unwrap_or_default(),
        }
    }
}

#[tokio::main]
async fn main() {
    let settings = Settings::from_the_environment();

    // The native messaging port is named by the environment the browser
    // sets; it is the only way into this process.
    let Ok(port) = std::env::var("CONDUIT_NATIVE_PORT") else {
        eprintln!("conduit-native-host: no native port in the environment");
        std::process::exit(2);
    };

    let Ok(native) = UnixStream::connect(&port).await else {
        eprintln!("conduit-native-host: the native port did not answer");
        std::process::exit(2);
    };

    if let Err(error) = carry(native, &settings).await {
        eprintln!("conduit-native-host: {error}");
        std::process::exit(1);
    }
}

/// Carry the tunnel: the native port on one side, the broker on the other.
async fn carry(mut native: UnixStream, settings: &Settings) -> std::io::Result<()> {
    let separator = if settings.broker.contains('?') {
        '&'
    } else {
        '?'
    };
    let url = format!(
        "{}{separator}role=extension&surface={}&secret={}",
        settings.broker,
        percent_encode(&settings.surface),
        percent_encode(&settings.secret)
    );

    let Ok((socket, _)) = tokio_tungstenite::connect_async(url.as_str()).await else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "the broker did not answer",
        ));
    };
    let (mut writer, mut reader) = socket.split();

    // The hello.
    let hello = Frame {
        kind: FrameType::Hello,
        schema: SchemaId::new("dev.conduit.hello", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: 0,
        payload: None,
    };
    let text = serde_json::to_string(&hello).unwrap_or_default();
    writer
        .send(tokio_tungstenite::tungstenite::Message::Text(text))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    // The ping, every twenty seconds: the worker on the other end of the
    // native port dies after thirty seconds of silence, and only traffic
    // that is flowing keeps it alive.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ping.tick() => {
                let pong = Frame {
                    kind: FrameType::Ping,
                    schema: SchemaId::new("dev.conduit.ping", 1),
                    channel: "tunnel".to_string(),
                    call: None,
                    seq: 0,
                    payload: None,
                };
                let text = serde_json::to_string(&pong).unwrap_or_default();
                if writer
                    .send(tokio_tungstenite::tungstenite::Message::Text(text))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            message = reader.next() => {
                let Some(Ok(message)) = message else { break };
                let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
                    continue;
                };
                let frame: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                let payload = serde_json::to_vec(&frame).unwrap_or_default();
                if write_native(&mut native, &payload).await.is_err() {
                    break;
                }
            }
            Some(message) = read_native(&mut native) => {
                let text = match message {
                    serde_json::Value::String(text) => text,
                    other => other.to_string(),
                };
                if writer
                    .send(tokio_tungstenite::tungstenite::Message::Text(text))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Read one message from the native port.
async fn read_native(native: &mut UnixStream) -> Option<serde_json::Value> {
    let mut length = [0u8; 4];
    if native.read_exact(&mut length).await.is_err() {
        return None;
    }
    let length = u32::from_be_bytes(length) as usize;
    if length > 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; length];
    if native.read_exact(&mut body).await.is_err() {
        return None;
    }
    serde_json::from_slice(&body).ok()
}

/// Write one message to the native port.
async fn write_native(native: &mut UnixStream, body: &[u8]) -> std::io::Result<()> {
    let length = (body.len() as u32).to_be_bytes();
    native.write_all(&length).await?;
    native.write_all(body).await?;
    native.flush().await
}

/// The percent encoding of a value in a query string.
fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
