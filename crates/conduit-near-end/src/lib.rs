//! The near end: a process that holds the socket of one or more surfaces.
//!
//! Every surface outside a browser needs the same thing: a process that
//! stays alive, holds a socket to the broker, and does the sealing.  A
//! VS Code extension host is killed when the window closes; a Slack
//! socket mode connection drops; an Apple Mail app extension is torn
//! down the moment the message is sent.  None of them can be trusted
//! with a socket, and none of them should be trusted with a key.
//!
//! So the near end is a separate process.  The plugin inside the app
//! speaks to it over a Unix socket, in plain JSON, one message at a
//! time:
//!
//! ```text
//! plugin  ->  {"op":"publish","schema":"dev.conduit.browser.tab/v1","payload":{…}}
//! near end ->  {"op":"event","schema":"…","channel":"…","seq":7,"payload":{…}}
//! ```
//!
//! What the plugin publishes is plaintext on that socket, and plaintext
//! on the socket to the broker is *not* what leaves: the near end seals
//! it under the key of the tunnel.  The plugin never sees a key.
//!
//! The near end does not know what the frames mean.  That is the
//! point: the surface decides what it publishes, and the near end only
//! carries it.

use conduit_protocol::end::{End, Role};
use conduit_protocol::{Frame, FrameType, SchemaId};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};

/// One surface the near end carries.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Surface {
    /// The identity of the surface, for example
    /// `dev.conduit.browser.tabs/v1`.
    pub id: String,
    /// The channel of the surface.  Defaults to the identity.
    #[serde(default)]
    pub channel: String,
    /// The schemas the surface may publish, for the record.
    #[serde(default)]
    pub schemas: Vec<String>,
    /// The scopes the surface needs, for the record.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// The Unix socket the plugin talks to.
    pub socket: PathBuf,
    /// The port on the loopback the plugin may talk to instead.
    ///
    /// A plugin that runs inside a web page cannot open a Unix socket,
    /// so the near end also listens on the loopback and speaks the same
    /// protocol there.
    #[serde(default)]
    pub tcp: Option<u16>,
    /// The secret of the grant, for the instance of this surface.
    #[serde(default)]
    pub secret: String,
}

impl Surface {
    /// The channel of the surface, which is the identity unless the
    /// configuration says otherwise.
    pub fn channel(&self) -> &str {
        if self.channel.is_empty() {
            &self.id
        } else {
            &self.channel
        }
    }
}

/// The configuration of the near end.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Where the broker is.
    pub broker: String,
    /// The instance of the tunnel: `extension` for a near end.
    #[serde(default = "the_default_instance")]
    pub instance: String,
    /// The surfaces to carry.
    pub surfaces: Vec<Surface>,
}

/// The instance of a near end.
fn the_default_instance() -> String {
    "extension".to_string()
}

/// What the plugin says to the near end.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum FromPlugin {
    /// Say where the broker is.
    ///
    /// Only the native mode needs this: a browser's native messaging
    /// host is started by the browser, which knows the broker and the
    /// secret the user approved, and the near end does not.
    Connect {
        broker: String,
        secret: String,
        instance: String,
    },
    /// Publish what the app knows.
    Publish { schema: String, payload: Value },
    /// Ask how the tunnel is.
    Status,
    /// A liveness check.
    Ping,
}

/// What the near end says to the plugin.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ToPlugin {
    /// A frame from the client, opened.
    Event {
        schema: String,
        channel: String,
        seq: u64,
        payload: Value,
    },
    /// How the tunnel is.
    Status {
        surface: String,
        agreed: bool,
        connected: bool,
        seq: u64,
    },
    /// The liveness check, answered.
    Pong,
    /// Something went wrong.
    Error { message: String },
}

/// The state of one surface's tunnel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// Whether the two ends have met.
    pub agreed: bool,
    /// Whether the socket to the broker is up.
    pub connected: bool,
    /// The sequence number of the last frame sent.
    pub seq: u64,
}

/// Carry every surface of the configuration.
pub async fn run(config: Config) -> std::io::Result<()> {
    let mut tasks = Vec::new();
    for surface in config.surfaces {
        let broker = config.broker.clone();
        let instance = config.instance.clone();
        let secret = surface.secret.clone();
        let path = surface.socket.clone();
        let port = surface.tcp;
        tasks.push(tokio::spawn(async move {
            carry(
                surface,
                broker,
                secret,
                instance,
                PluginSide::Unix(path),
                port,
            )
            .await
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
    Ok(())
}

/// Where the plugins of a surface are.
///
/// A surface is carried either by a socket on the machine or by the
/// standard input and output of the process: the second is what a
/// browser's native messaging host is, which is a pair of pipes.
pub enum PluginSide {
    /// A socket on the machine.
    Unix(PathBuf),
    /// The standard input and output of this process.
    Stdio,
}

/// Carry one surface: the broker on one side, the plugins on the other.
async fn carry(
    surface: Surface,
    broker: String,
    secret: String,
    instance: String,
    side: PluginSide,
    tcp: Option<u16>,
) -> std::io::Result<()> {
    let (events, _) = broadcast::channel::<ToPlugin>(256);

    // What the plugins publish, on its way to the broker.
    let (publishes, mut from_plugins) = mpsc::channel::<FromPlugin>(64);

    match &side {
        PluginSide::Unix(path) => {
            // A socket left over from a run that did not end well is
            // removed first: otherwise the bind fails and the surface
            // is dead for no reason.
            if path.exists() {
                let _ = std::fs::remove_file(path);
            }
            let listener = UnixListener::bind(path).map_err(|error| {
                std::io::Error::other(format!(
                    "the socket {} could not be bound ({error})",
                    path.display()
                ))
            })?;
            let accept_events = events.clone();
            let accept_publishes = publishes.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else {
                        break;
                    };
                    let events = accept_events.clone();
                    let publishes = accept_publishes.clone();
                    tokio::spawn(async move {
                        serve_plugin(stream, events, publishes).await;
                    });
                }
            });
        }
        PluginSide::Stdio => {
            let out_events = events.clone();
            let out_publishes = publishes.clone();
            tokio::spawn(async move {
                serve_stdio(out_events, out_publishes).await;
            });
        }
    }

    // The loopback, for the plugins that cannot open a Unix socket: a
    // web extension, a Figma iframe, a panel in an editor built on a
    // web page.  The protocol is the same, but a web socket frames the
    // messages itself.
    if let Some(port) = tcp {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .map_err(|error| {
                std::io::Error::other(format!("the port {port} could not be bound ({error})"))
            })?;
        let accept_events = events.clone();
        let accept_publishes = publishes.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let events = accept_events.clone();
                let publishes = accept_publishes.clone();
                tokio::spawn(async move {
                    serve_web(stream, events, publishes).await;
                });
            }
        });
    }

    // The broker.  In the native mode the near end does not know it
    // yet: the browser says where it is, with the secret the user
    // approved.
    let (broker, secret, instance) = if broker.is_empty() {
        let mut broker = String::new();
        let mut secret = String::new();
        let mut instance = String::new();
        while broker.is_empty() {
            let Some(message) = from_plugins.recv().await else {
                return Ok(());
            };
            if let FromPlugin::Connect {
                broker: said,
                secret: said_secret,
                instance: said_instance,
            } = message
            {
                broker = said;
                secret = said_secret;
                instance = said_instance;
            }
        }
        (broker, secret, instance)
    } else {
        (broker, secret, instance)
    };

    let separator = if broker.contains('?') { '&' } else { '?' };
    let url = format!(
        "{}{separator}role=extension&surface={}&secret={}&instance={}",
        broker,
        percent_encode(&surface.id),
        percent_encode(&secret),
        percent_encode(&instance)
    );
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| {
            std::io::Error::other(format!(
                "the broker did not answer ({error}); the url was {url}"
            ))
        })?;
    let (mut writer, mut reader) = socket.split();

    let mut end = End::new(Role::Extension);
    let hello = end.hello(&surface.id, surface.channel());
    let text = serde_json::to_string(&hello).unwrap_or_default();
    writer
        .send(tokio_tungstenite::tungstenite::Message::Text(text))
        .await
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    // The ping, every twenty seconds.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let connected = true;
    let mut state = State {
        agreed: false,
        connected: true,
        seq: 0,
    };

    // The plugins are told the tunnel is up, before the keys are
    // agreed: a plugin that waits for this does not race the broker.
    let _ = events.send(ToPlugin::Status {
        surface: surface.id.clone(),
        agreed: false,
        connected,
        seq: 0,
    });

    loop {
        tokio::select! {
            _ = ping.tick() => {
                let pong = Frame {
                    kind: FrameType::Ping,
                    schema: SchemaId::new("dev.conduit.ping", 1),
                    channel: surface.channel().to_string(),
                    call: None,
                    seq: 0,
                    payload: None,
                };
                let text = serde_json::to_string(&pong).unwrap_or_default();
                if writer.send(tokio_tungstenite::tungstenite::Message::Text(text)).await.is_err() {
                    break;
                }
            }
            message = reader.next() => {
                let Some(Ok(message)) = message else { break };
                let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
                    continue;
                };
                let frame: Value = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                // The hello of the client carries its public key.
                if frame["t"] == "hello" {
                    if let Some(payload) = frame["p"].as_str() {
                        if end.meet(payload).is_ok() {
                            state.agreed = true;
                            let _ = events.send(ToPlugin::Status {
                                surface: surface.id.clone(),
                                agreed: true,
                                connected,
                                seq: state.seq,
                            });
                        }
                    }
                    continue;
                }
                // A ping is not for the plugins.
                if frame["t"] == "ping" {
                    continue;
                }
                // Anything else is opened under the key of the tunnel and
                // handed to the plugins as it was.
                let schema = frame["s"].as_str().unwrap_or_default().to_string();
                let channel = frame["c"].as_str().unwrap_or_default().to_string();
                let sequence = frame["seq"].as_u64().unwrap_or(0);
                let payload = match frame["p"].as_str() {
                    Some(blob) => match end.open(blob, sequence) {
                        Ok(opened) => serde_json::from_slice(&opened)
                            .unwrap_or(Value::String(String::from_utf8_lossy(&opened).to_string())),
                        Err(_) => continue,
                    },
                    None => Value::Null,
                };
                let _ = events.send(ToPlugin::Event {
                    schema,
                    channel,
                    seq: sequence,
                    payload,
                });
            }
            Some(message) = from_plugins.recv() => {
                match message {
                    FromPlugin::Connect { .. } => {
                        let _ = events.send(ToPlugin::Error {
                            message: "the tunnel is already up".to_string(),
                        });
                    }
                    FromPlugin::Publish { schema, payload } => {
                        let body = serde_json::to_string(&payload).unwrap_or_default();
                        let Ok((sequence, blob)) = end.seal(body.as_bytes()) else {
                            let _ = events.send(ToPlugin::Error {
                                message: "the tunnel has no key yet".to_string(),
                            });
                            continue;
                        };
                        let Some(schema) = SchemaId::parse_or_first(&schema) else {
                            let _ = events.send(ToPlugin::Error {
                                message: format!("that is not a schema id: {schema}"),
                            });
                            continue;
                        };
                        let frame = Frame {
                            kind: FrameType::Publish,
                            schema,
                            channel: surface.channel().to_string(),
                            call: None,
                            seq: sequence,
                            payload: Some(blob),
                        };
                        state.seq = sequence;
                        let text = serde_json::to_string(&frame).unwrap_or_default();
                        if writer.send(tokio_tungstenite::tungstenite::Message::Text(text)).await.is_err() {
                            break;
                        }
                    }
                    FromPlugin::Status => {
                        let _ = events.send(ToPlugin::Status {
                            surface: surface.id.clone(),
                            agreed: state.agreed,
                            connected,
                            seq: state.seq,
                        });
                    }
                    FromPlugin::Ping => {
                        let _ = events.send(ToPlugin::Pong);
                    }
                }
            }
        }
    }
    if let PluginSide::Unix(path) = &side {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Serve the plugins that speak over the standard input and output.
///
/// This is what a browser's native messaging host is: the browser
/// writes to the standard input of this process and reads what comes
/// out of the standard output.  The framing is the same as the socket's:
/// a four-byte big-endian length, then that many bytes of JSON.
async fn serve_stdio(events: broadcast::Sender<ToPlugin>, publishes: mpsc::Sender<FromPlugin>) {
    let mut reader = tokio::io::BufReader::new(tokio::io::stdin());
    let mut writer = tokio::io::stdout();

    // What the browser says.
    let reader_publishes = publishes.clone();
    tokio::spawn(async move {
        while let Some(body) = read_frame(&mut reader).await {
            let message: FromPlugin = match serde_json::from_slice(&body) {
                Ok(message) => message,
                Err(_) => continue,
            };
            if reader_publishes.send(message).await.is_err() {
                break;
            }
        }
    });

    // What the browser is told.
    let mut events = events.subscribe();
    loop {
        let message = match events.recv().await {
            Ok(message) => message,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let body = serde_json::to_vec(&message).unwrap_or_default();
        if write_frame(&mut writer, &body).await.is_err() {
            break;
        }
    }
}

/// Serve one plugin: read what it says, tell it what happens.
///
/// Only one task ever writes to the plugin, so the messages cannot be
/// written over one another.
async fn serve_plugin(
    stream: UnixStream,
    events: broadcast::Sender<ToPlugin>,
    publishes: mpsc::Sender<FromPlugin>,
) {
    let (mut reader, mut writer) = stream.into_split();
    let (outgoing, mut outbox) = mpsc::channel::<ToPlugin>(64);

    // The writer.
    tokio::spawn(async move {
        while let Some(message) = outbox.recv().await {
            let body = serde_json::to_vec(&message).unwrap_or_default();
            if write_frame(&mut writer, &body).await.is_err() {
                break;
            }
        }
    });

    // The events, as they turn up.
    let mut events = events.subscribe();
    let event_out = outgoing.clone();
    tokio::spawn(async move {
        loop {
            let message = match events.recv().await {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            if event_out.send(message).await.is_err() {
                break;
            }
        }
    });

    // What the plugin says.
    loop {
        let Some(body) = read_frame(&mut reader).await else {
            break;
        };
        let message: FromPlugin = match serde_json::from_slice(&body) {
            Ok(message) => message,
            Err(error) => {
                eprintln!(
                    "DEBUG unreadable message: {} ({error})",
                    String::from_utf8_lossy(&body)
                );
                let error = ToPlugin::Error {
                    message: "that was not a message the near end knows".to_string(),
                };
                if outgoing.send(error).await.is_err() {
                    break;
                }
                continue;
            }
        };
        if publishes.send(message).await.is_err() {
            break;
        }
    }
}

/// Serve one plugin that came in over the loopback.
///
/// The protocol is the same, but a web socket frames the messages
/// itself, so there is no length in front of them.
async fn serve_web(
    stream: tokio::net::TcpStream,
    events: broadcast::Sender<ToPlugin>,
    publishes: mpsc::Sender<FromPlugin>,
) {
    use futures_util::stream::StreamExt as _;

    let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut writer, mut reader) = socket.split();

    // The events.
    let mut events = events.subscribe();
    tokio::spawn(async move {
        loop {
            let message = match events.recv().await {
                Ok(message) => message,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => break,
            };
            let body = serde_json::to_string(&message).unwrap_or_default();
            if writer
                .send(tokio_tungstenite::tungstenite::Message::Text(body))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // What the plugin says.
    while let Some(Ok(message)) = reader.next().await {
        let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
            continue;
        };
        let message: FromPlugin = match serde_json::from_str(&text) {
            Ok(message) => message,
            Err(_) => continue,
        };
        if publishes.send(message).await.is_err() {
            break;
        }
    }
}

/// Read one message: a four-byte big-endian length, then that many bytes.
async fn read_frame<R>(reader: &mut R) -> Option<Vec<u8>>
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
    Some(body)
}

/// Write one message.
async fn write_frame<W>(writer: &mut W, body: &[u8]) -> std::io::Result<()>
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

/// Carry one surface over the standard input and output.
///
/// This is the native messaging host of a browser: the browser starts
/// this process, writes to its standard input and reads what comes out
/// of its standard output.  The near end is told where the broker is
/// by the browser itself, in a `connect` message.
pub async fn run_native(config: Config) -> std::io::Result<()> {
    let Some(surface) = config.surfaces.into_iter().next() else {
        return Err(std::io::Error::other("the configuration names no surface"));
    };
    let id = surface.id.clone();
    carry(
        surface,
        String::new(),
        String::new(),
        config.instance,
        PluginSide::Stdio,
        None,
    )
    .await
    .map_err(|error| {
        std::io::Error::other(format!("the surface {id} could not be carried ({error})"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_surface_without_a_channel_uses_its_identity() {
        let surface = Surface {
            id: "dev.conduit.browser.tabs/v1".to_string(),
            channel: String::new(),
            schemas: vec![],
            scopes: vec![],
            socket: PathBuf::from("/tmp/x.sock"),
            tcp: None,
            secret: String::new(),
        };
        assert_eq!(surface.channel(), "dev.conduit.browser.tabs/v1");
    }

    #[test]
    fn the_configuration_is_read_from_json() {
        let json = r#"{
            "broker": "ws://127.0.0.1:8787/v1/tunnel",
            "surfaces": [
                {"id": "dev.conduit.browser.tabs/v1", "socket": "/tmp/tabs.sock"}
            ]
        }"#;
        let config: Config = serde_json::from_str(json).expect("the configuration is JSON");
        assert_eq!(config.instance, "extension");
        assert_eq!(config.surfaces.len(), 1);
        assert_eq!(config.surfaces[0].socket, PathBuf::from("/tmp/tabs.sock"));
    }

    #[test]
    fn the_local_protocol_has_an_op_in_every_message() {
        let message: FromPlugin =
            serde_json::from_str(r#"{"op":"publish","schema":"a/v1","payload":{"x":1}}"#)
                .expect("the message is JSON");
        match message {
            FromPlugin::Publish { schema, payload } => {
                assert_eq!(schema, "a/v1");
                assert_eq!(payload["x"], 1);
            }
            other => panic!("that was not a publish: {other:?}"),
        }
        let text = serde_json::to_string(&ToPlugin::Pong).expect("the message is JSON");
        assert_eq!(text, r#"{"op":"pong"}"#);
    }
}
