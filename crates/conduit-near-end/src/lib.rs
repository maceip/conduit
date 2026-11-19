//! The near end: a process that holds the socket of one or more surfaces.
//!
//! Every surface outside a browser needs the same thing: a process that
//! stays alive, holds a socket to the broker, and does the sealing.  A
//! VS Code extension host is killed when the window closes; a Slack
//! socket mode connection drops; an Apple Mail app extension is torn
//! down the moment the message is sent.  None of them can be trusted
//! with a socket, and none of them should be trusted with a key.
//!
//! So the near end is a separate process, and the plugin inside the app
//! speaks to it over a Unix socket, over the loopback, or over the
//! standard input and output of the process — which is what a browser's
//! native messaging host is.
//!
//! **The hop between a plugin and the near end carries the same frames
//! the tunnel carries.**  There is no second vocabulary.  A plugin
//! publishes a frame whose payload is plaintext; the near end seals the
//! payload and sends the frame on; a frame that comes back is opened
//! and handed to the plugin as it was.  The near end is a seal adapter,
//! not a translator.
//!
//! There is one frame kind that the plugin may send and the tunnel
//! does not have: `connect`, which says where the broker is and what
//! secret may be presented on it.  It is how a plugin that did the
//! device flow hands the result to the process that holds the socket.
//!
//! A plugin does not wait for the tunnel to be up.  The near end holds
//! what a plugin publishes until the two ends have agreed their keys,
//! and then sends it.  The waiting happens once, here, and not in every
//! plugin in every language.

use conduit_protocol::end::{End, Role};
use conduit_protocol::{Frame, FrameType, SchemaId};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
    #[serde(default)]
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
    /// Where the broker is.  A plugin may say otherwise with a
    /// `connect` frame.
    #[serde(default)]
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

impl Config {
    /// The secret of the first surface, for the native mode.
    fn surfaces_secret(&self) -> String {
        self.surfaces
            .first()
            .map(|surface| surface.secret.clone())
            .unwrap_or_default()
    }
}

/// Where the plugins of a surface are.
///
/// A surface is carried either by a socket on the machine, by a socket
/// on the loopback, or by the standard input and output of the process:
/// the third is what a browser's native messaging host is, which is a
/// pair of pipes.
pub enum PluginSide {
    /// A socket on the machine.
    Unix(PathBuf),
    /// A socket on the loopback, for a plugin in a web page.
    Loopback(u16),
    /// The standard input and output of this process.
    Stdio,
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
        // A surface is carried on a socket on the machine, and on the
        // loopback when the declaration says so: a plugin in a web
        // page cannot open a socket on the machine.
        let mut sides = vec![PluginSide::Unix(path)];
        if let Some(port) = port {
            sides.push(PluginSide::Loopback(port));
        }
        tasks.push(tokio::spawn(async move {
            carry(surface, broker, secret, instance, sides).await
        }));
    }
    for task in tasks {
        let _ = task.await;
    }
    Ok(())
}

/// Carry one surface over the standard input and output.
///
/// This is the mode a browser's native messaging host uses: the browser
/// starts the process and speaks the local hop over its pipes.  The
/// configuration may be empty, because the plugin says which surface it
/// speaks for in its `connect` frame.
pub async fn run_native(config: Config) -> std::io::Result<()> {
    let secret = config.surfaces_secret();
    let surface = config.surfaces.into_iter().next().unwrap_or(Surface {
        id: String::new(),
        channel: String::new(),
        schemas: vec![],
        scopes: vec![],
        socket: PathBuf::new(),
        tcp: None,
        secret: String::new(),
    });
    carry(
        surface,
        config.broker,
        secret,
        config.instance,
        vec![PluginSide::Stdio],
    )
    .await
}

/// What the plugins of one surface share.
///
/// The frames that come back from the broker are broadcast to every
/// plugin.  The welcome is also state: a plugin that connects after
/// the tunnel is up is told so, rather than waiting for a frame that
/// has already gone by.
#[derive(Clone)]
struct Shared {
    /// The frames that come back from the broker.
    events: broadcast::Sender<Frame>,
    /// The welcome, once the broker has sent it.
    welcome: Arc<Mutex<Option<Frame>>>,
}

impl Shared {
    /// Remember the welcome, and tell everybody who is listening.
    fn welcome(&self, frame: Frame) {
        if let Ok(mut welcome) = self.welcome.lock() {
            *welcome = Some(frame.clone());
        }
        let _ = self.events.send(frame);
    }
}

/// Carry one surface: the broker on one side, the plugins on the other.
async fn carry(
    mut surface: Surface,
    broker: String,
    secret: String,
    instance: String,
    sides: Vec<PluginSide>,
) -> std::io::Result<()> {
    let (events, _) = broadcast::channel::<Frame>(256);
    let shared = Shared {
        events,
        welcome: Arc::new(Mutex::new(None)),
    };

    // What the plugins publish, on its way to the broker.
    let (publishes, mut from_plugins) = mpsc::channel::<Frame>(64);

    for side in &sides {
        match side {
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
                let accept_events = shared.clone();
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
                let out_events = shared.clone();
                let out_publishes = publishes.clone();
                tokio::spawn(async move {
                    serve_stdio(out_events, out_publishes).await;
                });
            }
            PluginSide::Loopback(port) => {
                let listener = tokio::net::TcpListener::bind(("127.0.0.1", *port))
                    .await
                    .map_err(|error| {
                        std::io::Error::other(format!(
                            "the port {port} could not be bound ({error})"
                        ))
                    })?;
                let accept_events = shared.clone();
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
        }
    }

    // The broker.  A plugin may say where it is, in a `connect` frame;
    // the configuration is the answer until then.
    let mut broker = broker;
    let mut secret = secret;
    let mut instance = instance;

    // The keys of the tunnel.
    let mut end = End::new(Role::Extension);
    let mut writer: Option<Writer> = None;
    let mut reader: Option<Reader> = None;

    // What the plugins published before the tunnel was up.
    let mut held: Vec<Frame> = Vec::new();

    let mut state = State {
        agreed: false,
        connected: false,
        seq: 0,
    };

    // The ping, every twenty seconds.
    let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    // The attempt to reach the broker.  The near end is a long-lived
    // process: the broker may not be there yet, and the network may
    // drop.  Neither is a reason to die.
    let mut retry = tokio::time::interval(std::time::Duration::from_millis(500));
    retry.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = retry.tick(), if writer.is_none() => {
                if broker.is_empty() {
                    continue;
                }
                match open_the_tunnel(&broker, &surface, &secret, &instance, &mut end).await {
                    Ok((new_writer, new_reader)) => {
                        writer = Some(new_writer);
                        reader = Some(new_reader);
                        state.connected = true;
                    }
                    Err(_) => {
                        // The broker is not there yet.  Try again.
                    }
                }
            }
            _ = ping.tick() => {
                if let Some(writer) = writer.as_mut() {
                    let pong = Frame {
                        kind: FrameType::Ping,
                        schema: SchemaId::new("dev.conduit.ping", 1),
                        channel: surface.channel().to_string(),
                        call: None,
                        seq: 0,
                        payload: None,
                    };
                    let text = serde_json::to_string(&pong).unwrap_or_default();
                    if writer.send(text).await.is_err() {
                        break;
                    }
                }
            }
            message = read_from_broker(&mut reader), if reader.is_some() => {
                let Some(text) = message else {
                    // The connection is gone.  The tunnel is over: a
                    // new one needs new keys, and everything published
                    // so far waits again.
                    writer = None;
                    reader = None;
                    state.agreed = false;
                    state.connected = false;
                    end = End::new(Role::Extension);
                    continue;
                };
                let mut frame: Frame = match serde_json::from_str(&text) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                };
                // The hello of the client carries its public key.
                if frame.kind == FrameType::Hello {
                    if let Some(payload) = frame.payload.as_ref() {
                        if end.meet(payload).is_ok() {
                            state.agreed = true;
                            // What the plugins published while there was
                            // no key to seal it under goes now.
                            let waiting = std::mem::take(&mut held);
                            for frame in waiting {
                                if send_to_broker(&mut writer, &surface, &mut end, frame, &mut state)
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                    continue;
                }
                // The broker's welcome: the tunnel is up.  The plugins
                // are told, so a popup can say so.
                if frame.kind == FrameType::Welcome {
                    state.connected = true;
                    shared.welcome(frame);
                    continue;
                }
                // A ping is for the near end, not for the plugins.
                if frame.kind == FrameType::Ping {
                    continue;
                }
                // The broker's own traffic is not the plugins'
                // business: the subscription of the client is between
                // the client and the broker.
                if !matches!(
                    frame.kind,
                    FrameType::Publish | FrameType::Result | FrameType::Error
                ) {
                    continue;
                }
                // What is left is opened under the key of the tunnel and
                // handed to the plugins as it was.
                if let Some(payload) = frame.payload.as_ref() {
                    let blob = payload.as_str().unwrap_or_default().to_string();
                    if let Ok(opened) = end.open(&blob, frame.seq) {
                        frame.payload = Some(
                            serde_json::from_slice(&opened).unwrap_or(Value::String(
                                String::from_utf8_lossy(&opened).to_string(),
                            )),
                        );
                    }
                }
                let _ = shared.events.send(frame);
            }
            Some(frame) = from_plugins.recv() => {
                // A plugin may say where the broker is.
                if frame.kind == FrameType::Connect {
                    if let Some(payload) = frame.payload.as_ref() {
                        // The plugin may say which surface it speaks
                        // for: in the native mode there is no
                        // configuration to say it.
                        if let Some(id) = payload.get("surface").and_then(|value| value.as_str()) {
                            surface.id = id.to_string();
                        }
                        if let Some(channel) =
                            payload.get("channel").and_then(|value| value.as_str())
                        {
                            surface.channel = channel.to_string();
                        }
                        if let Some(said) = payload.get("broker").and_then(|value| value.as_str()) {
                            broker = said.to_string();
                            secret = payload
                                .get("secret")
                                .and_then(|value| value.as_str())
                                .unwrap_or_default()
                                .to_string();
                            instance = payload
                                .get("instance")
                                .and_then(|value| value.as_str())
                                .unwrap_or("extension")
                                .to_string();
                            // The connection is made by the retry
                            // branch, which tries again until the
                            // broker answers.
                            writer = None;
                            reader = None;
                            state.agreed = false;
                            state.connected = false;
                            end = End::new(Role::Extension);
                            continue;
                        }
                    }
                    continue;
                }
                // Only a publish is a publish: a plugin may not send
                // anything else.
                if frame.kind != FrameType::Publish {
                    continue;
                }
                if !state.agreed {
                    // There is no key to seal it under yet.  It waits
                    // here, and the plugin does not have to know.
                    held.push(frame);
                    continue;
                }
                if send_to_broker(&mut writer, &surface, &mut end, frame, &mut state)
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

/// Open the tunnel to the broker.
async fn open_the_tunnel(
    broker: &str,
    surface: &Surface,
    secret: &str,
    instance: &str,
    end: &mut End,
) -> std::io::Result<(Writer, Reader)> {
    let separator = if broker.contains('?') { '&' } else { '?' };
    let url = format!(
        "{}{separator}role=extension&surface={}&secret={}&instance={}",
        broker,
        percent_encode(&surface.id),
        percent_encode(secret),
        percent_encode(instance)
    );
    let (socket, _) = tokio_tungstenite::connect_async(url.as_str())
        .await
        .map_err(|error| {
            std::io::Error::other(format!(
                "the broker did not answer ({error}); the url was {url}"
            ))
        })?;
    let (writer, reader) = socket.split();
    let mut writer = Writer { socket: writer };
    let reader = Reader { socket: reader };

    // The hello, with the public key of this end.
    let hello = end.hello(&surface.id, surface.channel());
    let text = serde_json::to_string(&hello).unwrap_or_default();
    writer.send(text).await?;
    Ok((writer, reader))
}

/// Seal a frame and send it to the broker.
async fn send_to_broker(
    writer: &mut Option<Writer>,
    surface: &Surface,
    end: &mut End,
    mut frame: Frame,
    state: &mut State,
) -> std::io::Result<()> {
    let Some(writer) = writer.as_mut() else {
        return Ok(());
    };
    let payload = frame.payload.take().unwrap_or(Value::Null);
    let body = payload.to_string();
    let Ok((sequence, blob)) = end.seal(body.as_bytes()) else {
        return Ok(());
    };
    frame.seq = sequence;
    frame.payload = Some(Value::String(blob));
    frame.channel = surface.channel().to_string();
    state.seq = sequence;
    let text = serde_json::to_string(&frame).unwrap_or_default();
    writer.send(text).await
}

/// The writing half of the socket to the broker.
struct Writer {
    socket: futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        tokio_tungstenite::tungstenite::Message,
    >,
}

impl Writer {
    /// Send one frame.
    async fn send(&mut self, text: String) -> std::io::Result<()> {
        self.socket
            .send(tokio_tungstenite::tungstenite::Message::Text(text))
            .await
            .map_err(|error| std::io::Error::other(error.to_string()))
    }
}

/// The reading half of the socket to the broker.
struct Reader {
    socket: futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
}

/// Read one frame from the broker, if there is one waiting.
async fn read_from_broker(reader: &mut Option<Reader>) -> Option<String> {
    let reader = reader.as_mut()?;
    let message = reader.socket.next().await?;
    match message {
        Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => Some(text),
        _ => None,
    }
}

/// Serve one plugin: read what it says, tell it what happens.
async fn serve_plugin(stream: UnixStream, shared: Shared, publishes: mpsc::Sender<Frame>) {
    let (mut reader, mut writer) = stream.into_split();
    let (outgoing, mut outbox) = mpsc::channel::<Frame>(64);

    // The writer.
    tokio::spawn(async move {
        while let Some(frame) = outbox.recv().await {
            let body = serde_json::to_vec(&frame).unwrap_or_default();
            if write_frame(&mut writer, &body).await.is_err() {
                break;
            }
        }
    });

    // The tunnel may already be up: a plugin that comes late is told.
    if let Some(welcome) = the_welcome(&shared) {
        let _ = outgoing.send(welcome).await;
    }

    // The events, as they turn up.
    let mut events = shared.events.subscribe();
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
        let frame: Frame = match serde_json::from_slice(&body) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        if publishes.send(frame).await.is_err() {
            break;
        }
    }
}

/// Serve a plugin that came in over the loopback.
///
/// The protocol is the same, but a web socket frames the messages
/// itself, so there is no length in front of them.
async fn serve_web(stream: tokio::net::TcpStream, shared: Shared, publishes: mpsc::Sender<Frame>) {
    let Ok(socket) = tokio_tungstenite::accept_async(stream).await else {
        return;
    };
    let (mut writer, mut reader) = socket.split();
    let (outgoing, mut outbox) = mpsc::channel::<Frame>(64);

    tokio::spawn(async move {
        while let Some(frame) = outbox.recv().await {
            let text = serde_json::to_string(&frame).unwrap_or_default();
            if writer
                .send(tokio_tungstenite::tungstenite::Message::Text(text))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    // The tunnel may already be up: a plugin that comes late is told.
    if let Some(welcome) = the_welcome(&shared) {
        let _ = outgoing.send(welcome).await;
    }

    let mut events = shared.events.subscribe();
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

    while let Some(Ok(message)) = reader.next().await {
        let tokio_tungstenite::tungstenite::Message::Text(text) = message else {
            continue;
        };
        let frame: Frame = match serde_json::from_str(&text) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        if publishes.send(frame).await.is_err() {
            break;
        }
    }
}

/// Serve the plugins that speak over the standard input and output.
///
/// This is what a browser's native messaging host is: the browser
/// writes to the standard input of this process and reads what comes
/// out of the standard output.  The framing is the same as the socket's:
/// a four-byte big-endian length, then that many bytes of JSON.
///
/// The standard input and the standard output of a process are pipes,
/// and a read of one blocks.  Tokio's own `stdin` blocks the worker
/// thread it runs on, which on a small machine starves the tunnel: two
/// workers blocked on the pipes, and nothing left to carry the frames.
/// So both ends are read and written on the blocking pool, and the
/// tunnel keeps its workers.
async fn serve_stdio(shared: Shared, publishes: mpsc::Sender<Frame>) {
    use std::io::Read;

    // What the browser says.
    let (said, mut from_browser) = mpsc::channel::<Vec<u8>>(64);
    tokio::task::spawn_blocking(move || {
        let mut input = std::io::stdin();
        let mut buffer: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match input.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => buffer.extend_from_slice(&chunk[..read]),
                Err(_) => break,
            }
            // Take the frames out of what has come in.
            loop {
                if buffer.len() < 4 {
                    break;
                }
                let length =
                    u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]) as usize;
                if length > 1024 * 1024 || buffer.len() < 4 + length {
                    break;
                }
                let frame = buffer[4..4 + length].to_vec();
                buffer.drain(..4 + length);
                if said.blocking_send(frame).is_err() {
                    return;
                }
            }
        }
    });

    let reader_publishes = publishes.clone();
    tokio::spawn(async move {
        while let Some(body) = from_browser.recv().await {
            let frame: Frame = match serde_json::from_slice(&body) {
                Ok(frame) => frame,
                Err(_) => continue,
            };
            if reader_publishes.send(frame).await.is_err() {
                break;
            }
        }
    });

    // What the browser is told.
    let (told, mut to_browser) = mpsc::channel::<Vec<u8>>(64);
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        let mut output = std::io::stdout();
        while let Some(body) = to_browser.blocking_recv() {
            let length = (body.len() as u32).to_be_bytes();
            if output.write_all(&length).is_err() {
                break;
            }
            if output.write_all(&body).is_err() {
                break;
            }
            if output.flush().is_err() {
                break;
            }
        }
    });

    let mut events = shared.events.subscribe();
    loop {
        let message = match events.recv().await {
            Ok(message) => message,
            Err(broadcast::error::RecvError::Lagged(_)) => continue,
            Err(broadcast::error::RecvError::Closed) => break,
        };
        let body = serde_json::to_vec(&message).unwrap_or_default();
        if told.send(body).await.is_err() {
            break;
        }
    }
}

/// The welcome, if the tunnel is already up.
fn the_welcome(shared: &Shared) -> Option<Frame> {
    let welcome = shared.welcome.lock().ok()?;
    welcome.clone()
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
    fn a_plugin_publishes_a_frame_of_the_tunnels_own_shape() {
        let frame = Frame {
            kind: FrameType::Publish,
            schema: SchemaId::new("dev.conduit.browser.tab", 1),
            channel: "tabs".to_string(),
            call: None,
            seq: 0,
            payload: Some(serde_json::json!({"url": "https://example.test/"})),
        };
        let text = serde_json::to_string(&frame).expect("the frame is JSON");
        let back: Frame = serde_json::from_str(&text).expect("the frame comes back");
        assert_eq!(back.kind, FrameType::Publish);
        assert_eq!(back.payload.unwrap()["url"], "https://example.test/");
    }
}
