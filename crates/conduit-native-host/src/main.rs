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

use conduit_protocol::sealed::{self, KeyPair, SharedSecret};
use conduit_protocol::{Frame, FrameType, SchemaId};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// The configuration of the host, from the environment.
struct Settings {
    broker: String,
    surface: String,
    secret: String,
    instance: String,
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
            instance: std::env::var("CONDUIT_INSTANCE").unwrap_or_else(|_| "extension".to_string()),
        }
    }
}

#[tokio::main]
async fn main() {
    let settings = Settings::from_the_environment();

    // The native messaging port is named by the environment the browser
    // sets; it is the only way into this process.
    let Ok(port) = std::env::var("CONDUIT_NATIVE_PORT") else {
        eprintln!("conduit-native-host: no native port in env");
        std::process::exit(2);
    };

    let Ok(native) = UnixStream::connect(&port).await else {
        eprintln!("conduit-native-host: native port did not answer");
        std::process::exit(2);
    };

    if let Err(error) = carry(native, &settings).await {
        eprintln!("conduit-native-host: {error}");
        std::process::exit(1);
    }
}

/// The keys of the tunnel, once the two ends have met.
struct Keys {
    /// The key that seals what this end sends — the extension's.
    to_extension: [u8; 32],
    /// The key that opens what the client sends.
    to_client: [u8; 32],
    /// The prefix of the client's nonces.
    prefix_of_client: [u8; 4],
}

/// Carry the tunnel: the native port on one side, the broker on the other.
///
/// The host owns the key pair: the extension in Safari cannot keep one,
/// because its background context is unloaded, and the socket is here
/// anyway.
async fn carry(native: UnixStream, settings: &Settings) -> std::io::Result<()> {
    let separator = if settings.broker.contains('?') {
        '&'
    } else {
        '?'
    };
    let url = format!(
        "{}{separator}role=extension&surface={}&secret={}&instance={}",
        settings.broker,
        percent_encode(&settings.surface),
        percent_encode(&settings.secret),
        percent_encode(&settings.instance)
    );

    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| {
            std::io::Error::other(format!(
                "the broker did not answer ({error}); the url was {url}"
            ))
        })?;
    let (mut writer, mut reader) = socket.split();

    // The hello, with the public key of this end.
    let keys = KeyPair::generate();
    let prefix = sealed::random_prefix();
    let mut sequence: u64 = 0;
    let hello = Frame {
        kind: FrameType::Hello,
        schema: SchemaId::new("dev.conduit.hello", 1),
        channel: "tunnel".to_string(),
        call: None,
        seq: {
            sequence += 1;
            sequence
        },
        payload: Some(
            serde_json::json!({
                "role": "extension",
                "surface": settings.surface,
                "crypto": {
                    "scheme": sealed::SCHEME,
                    "public": sealed::to_wire(&keys.public_bytes()),
                    "prefix": sealed::to_wire(&prefix),
                },
            })
            .to_string(),
        ),
    };
    let text = serde_json::to_string(&hello).unwrap_or_default();
    writer
        .send(tokio_tungstenite::tungstenite::Message::Text(text))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    // The keys of the tunnel, once the client's hello has arrived.
    let mut tunnel: Option<Keys> = None;

    // The ping, every twenty seconds: the worker on the other end of the
    // native port dies after thirty seconds of silence, and only traffic
    // that is flowing keeps it alive.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // The native port is read and written by their own tasks.
    //
    // A frame on that port is a length and then bytes, and a read of
    // it is not safe to cancel: cancel it halfway and the bytes that
    // were already read are lost.  So the port has its own tasks, and
    // the loop below only ever waits on channels, which are safe to
    // drop.
    let (mut native_reader, mut native_writer) = native.into_split();
    let (to_broker, mut from_native) = tokio::sync::mpsc::channel::<Value>(64);
    let (to_extension, mut to_native) = tokio::sync::mpsc::channel::<Vec<u8>>(64);

    tokio::spawn(async move {
        while let Some(message) = read_native(&mut native_reader).await {
            if to_broker.send(message).await.is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(body) = to_native.recv().await {
            if write_native(&mut native_writer, &body).await.is_err() {
                break;
            }
        }
    });

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
                // The hello of the client carries its public key.
                if frame["t"] == "hello" {
                    if let Some(payload) = frame["p"].as_str() {
                        if let Some(shared) = meet(payload, &keys) {
                            tunnel = Some(shared);
                        }
                    }
                    continue;
                }
                // The payload is opened under the key of the tunnel, and
                // handed to the extension as it was.
                let mut frame = frame.clone();
                if let (Some(tunnel), Some(seq)) = (tunnel.as_ref(), frame["seq"].as_u64()) {
                    if let Some(blob) = frame["p"].as_str() {
                        if let Some(sealed) = sealed::Sealed::from_base64(blob) {
                            if let Some(opened) = sealed::open(
                                &tunnel.to_client,
                                tunnel.prefix_of_client,
                                seq,
                                &sealed,
                            ) {
                                frame["p"] =
                                    Value::String(String::from_utf8_lossy(&opened).to_string());
                            }
                        }
                    }
                }
                let payload = serde_json::to_vec(&frame).unwrap_or_default();
                if to_extension.send(payload).await.is_err() {
                    break;
                }
            }
            Some(message) = from_native.recv() => {
                // What the extension sends is plaintext: the host is the
                // one that seals it, under the key of the tunnel.
                let mut frame: Value = match message {
                    Value::String(text) => match serde_json::from_str(&text) {
                        Ok(frame) => frame,
                        Err(_) => continue,
                    },
                    other => other,
                };
                if let Some(tunnel) = tunnel.as_ref() {
                    sequence += 1;
                    if let Some(payload) = frame["p"].as_str() {
                        let sealed =
                            sealed::seal(&tunnel.to_extension, prefix, sequence, payload.as_bytes());
                        frame["seq"] = Value::from(sequence);
                        frame["p"] = Value::String(sealed.to_base64());
                    }
                }
                let text = serde_json::to_string(&frame).unwrap_or_default();
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
///
/// The port frames a message as a four-byte big-endian length and then
/// that many bytes of JSON.
async fn read_native<R>(reader: &mut R) -> Option<serde_json::Value>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut length = [0u8; 4];
    if reader.read_exact(&mut length).await.is_err() {
        return None;
    }
    let length = u32::from_be_bytes(length) as usize;
    if length > 1024 * 1024 {
        return None;
    }
    let mut body = vec![0u8; length];
    if reader.read_exact(&mut body).await.is_err() {
        return None;
    }
    serde_json::from_slice(&body).ok()
}

/// Write one message to the native port.
async fn write_native<W>(writer: &mut W, body: &[u8]) -> std::io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let length = (body.len() as u32).to_be_bytes();
    writer.write_all(&length).await?;
    writer.write_all(body).await?;
    writer.flush().await
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

/// Agree the keys of the tunnel with the client's hello.
///
/// The hello carries the peer's public key and the prefix of its
/// nonces, both base64url without padding — the same encoding the
/// browser uses, and the one the protocol names.
fn meet(payload: &str, keys: &KeyPair) -> Option<Keys> {
    let hello: serde_json::Value = serde_json::from_str(payload).ok()?;
    let crypto = hello.get("crypto")?;
    let public = crypto.get("public")?.as_str()?;
    let prefix = crypto.get("prefix")?.as_str()?;

    let public = sealed::from_wire(public)?;
    if public.len() != 65 {
        return None;
    }
    let mut point = [0u8; 65];
    point.copy_from_slice(&public);
    let shared: SharedSecret = keys.agree(&point);

    let prefix = sealed::from_wire(prefix)?;
    if prefix.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&prefix);
    Some(Keys {
        to_extension: shared.to_extension(),
        to_client: shared.to_client(),
        prefix_of_client: bytes,
    })
}
