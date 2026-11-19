//! The client: the far end of a tunnel.
//!
//! A client subscribes to a surface by its address, and receives the
//! frames the extension publishes under that surface.  It never sees the
//! app: the extension is on the other side of the tunnel, and the
//! payload arrives sealed.
//!
//! ```no_run
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use conduit_client::{Address, Client};
//! use conduit_protocol::{verb, Scope};
//!
//! let address = Address::parse("conduit://mail/compose@v1")?;
//! let mut client = Client::connect("ws://127.0.0.1:8787/v1/tunnel", "token", &address).await?;
//! client
//!     .subscribe(&address, &[Scope::new("mail.compose", verb::READ)])
//!     .await?;
//!
//! while let Some(event) = client.next().await? {
//!     println!("{}", event.payload);
//! }
//! # Ok(())
//! # }
//! ```

use conduit_protocol::end::{End, Role};
use conduit_protocol::{Frame, FrameType, SchemaId, Scope};
use futures_util::{SinkExt, StreamExt};
use std::collections::{HashMap, VecDeque};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// An address of a surface: `conduit://authority/path@version`.
///
/// The scheme says what kind of thing is addressed; the authority names
/// the surface; the path names the channel; the version pins the schema
/// set.  A client may hold one of these and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    /// The scheme of the address.
    pub scheme: String,
    /// The authority of the address.
    pub authority: String,
    /// The path of the address.
    pub path: String,
    /// The version of the schema set.
    pub version: u16,
}

impl Address {
    /// The address in that string, if it is one.
    pub fn parse(text: &str) -> Result<Address, AddressError> {
        let (scheme, rest) = text.split_once("://").ok_or(AddressError::NoScheme)?;
        if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_lowercase()) {
            return Err(AddressError::NoScheme);
        }
        let (authority, rest) = match rest.split_once('/') {
            Some((authority, rest)) => (authority, format!("/{rest}")),
            None => (rest, "/".to_string()),
        };
        if authority.is_empty() {
            return Err(AddressError::NoAuthority);
        }
        let (path, version) = match rest.rsplit_once('@') {
            Some((path, version)) => {
                let major = version
                    .strip_prefix('v')
                    .and_then(|number| number.parse::<u16>().ok())
                    .ok_or(AddressError::NoVersion)?;
                (path.to_string(), major)
            }
            None => (rest, 1),
        };
        Ok(Address {
            scheme: scheme.to_string(),
            authority: authority.to_string(),
            path,
            version,
        })
    }

    /// The surface the address names.
    pub fn surface(&self) -> SchemaId {
        SchemaId::new(&self.authority, self.version)
    }

    /// The channel the address names.
    pub fn channel(&self) -> String {
        self.path.trim_matches('/').replace('/', ".")
    }
}

/// Why an address could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressError {
    /// The address has no scheme.
    NoScheme,
    /// The address has no authority.
    NoAuthority,
    /// The address has no version.
    NoVersion,
}

impl std::fmt::Display for AddressError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddressError::NoScheme => formatter.write_str("address has no scheme"),
            AddressError::NoAuthority => formatter.write_str("address has no authority"),
            AddressError::NoVersion => formatter.write_str("address has no version"),
        }
    }
}

impl std::error::Error for AddressError {}

/// An event: a frame, with its payload opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The schema the payload follows.
    pub schema: SchemaId,
    /// The channel the event belongs to.
    pub channel: String,
    /// The sequence number of the event.
    pub seq: u64,
    /// The payload, opened.
    pub payload: String,
    /// The payload as it arrived, sealed.
    pub sealed: String,
}

/// What came in: an event, or a refusal (the whole error frame).
enum Incoming {
    Event(Event),
    Refused(String),
}

/// The keys of a tunnel, once the two ends have met.
///
/// The payload is sealed end to end: the broker sees the envelope and
/// nothing else.
#[derive(Clone)]
pub struct Keys {
    /// The key that seals what the client sends.
    pub to_extension: [u8; 32],
    /// The key that opens what the extension sends.
    pub to_extension_read: [u8; 32],
    /// The nonce prefix the extension uses.
    pub prefix_of_extension: [u8; 4],
}

/// A client: a connection to the broker.
pub struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    channels: HashMap<String, Vec<Scope>>,
    surface: SchemaId,
    /// The keys of this end, and the keys of the tunnel once the two
    /// ends have met.
    ///
    /// The client and the near end share one implementation of this:
    /// there is no second way to seal a payload, and the tests that
    /// compare them compare the same code.
    end: End,
    /// What has arrived and not yet been handed over: events, and the
    /// broker's refusals, in the order they came.
    pending: VecDeque<Incoming>,
}

impl Client {
    /// Connect to the broker and say who we are.
    ///
    /// The surface is part of the connection: a tunnel is one surface,
    /// and the broker pairs the ends of that surface with each other.
    pub async fn connect(url: &str, token: &str, address: &Address) -> Result<Client, ClientError> {
        let separator = if url.contains('?') { '&' } else { '?' };
        let surface = format!("{}/v{}", address.authority, address.version);
        let full = format!(
            "{url}{separator}role=client&token={token}&surface={}",
            percent_encode(&surface)
        );
        let request = full
            .as_str()
            .into_client_request()
            .map_err(|error| ClientError::Connection(Box::new(error)))?;
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|error| ClientError::Connection(Box::new(error)))?;

        let mut client = Client {
            socket,
            channels: HashMap::new(),
            surface: address.surface(),
            end: End::new(Role::Client),
            pending: VecDeque::new(),
        };
        // The hello, with the public key of this end.
        let hello = client.end.hello(&surface, &address.channel());
        client.send(&hello).await?;
        client.await_welcome().await?;
        Ok(client)
    }

    /// Wait for the broker's welcome.
    ///
    /// The broker refuses an end it does not want: the token does not
    /// cover the surface, or no extension is on it.  The refusal comes
    /// as an error frame and then the socket closes, so a connect that
    /// returned without the welcome would have told the caller
    /// nothing.
    async fn await_welcome(&mut self) -> Result<(), ClientError> {
        loop {
            let Some(Ok(message)) = self.socket.next().await else {
                return Err(ClientError::Connection(Box::new(
                    tokio_tungstenite::tungstenite::Error::AlreadyClosed,
                )));
            };
            let Message::Text(text) = message else {
                continue;
            };
            let frame: Frame = match serde_json::from_str(&text) {
                Ok(frame) => frame,
                Err(_) => continue,
            };
            match frame.kind {
                FrameType::Welcome => return Ok(()),
                FrameType::Error => return Err(ClientError::Refused(text)),
                _ => continue,
            }
        }
    }

    /// Subscribe to an address, with those scopes.
    pub async fn subscribe(
        &mut self,
        address: &Address,
        scopes: &[Scope],
    ) -> Result<Subscription, ClientError> {
        if address.surface() != self.surface {
            return Err(ClientError::WrongSurface {
                wanted: address.surface().to_string(),
                connected: self.surface.to_string(),
            });
        }
        self.pump().await?;
        let channel = address.channel();
        let payload = serde_json::json!({
            "channel": channel,
            "scopes": scopes.iter().map(|scope| scope.to_string()).collect::<Vec<_>>(),
        });
        let frame = Frame {
            kind: FrameType::Subscribe,
            schema: SchemaId::new("dev.conduit.subscribe", 1),
            channel: channel.clone(),
            call: None,
            seq: self.end.next_sequence(),
            payload: Some(payload),
        };
        self.send(&frame).await?;
        self.channels.insert(channel.clone(), scopes.to_vec());
        Ok(Subscription {
            channel,
            scopes: scopes.to_vec(),
        })
    }

    /// The next event of the tunnel, whatever channel it belongs to.
    pub async fn next(&mut self) -> Result<Option<Event>, ClientError> {
        loop {
            match self.pending.pop_front() {
                Some(Incoming::Event(event)) => return Ok(Some(event)),
                Some(Incoming::Refused(text)) => return Err(ClientError::Refused(text)),
                None => {}
            }
            let message = self.socket.next().await;
            let Some(message) = message else {
                return Ok(None);
            };
            let message = message.map_err(|error| ClientError::Connection(Box::new(error)))?;
            let Message::Text(text) = message else {
                continue;
            };
            if let Some(incoming) = self.take(&text)? {
                self.pending.push_back(incoming);
            }
        }
    }

    /// Read the frames that have already arrived, without waiting.
    ///
    /// The hello of the extension carries its public key, and the client
    /// cannot publish anything until it has that key; so the frames that
    /// are waiting are read before anything is sent.  A refusal read here
    /// answers an earlier frame => it's queued for `next`, not returned as
    /// the failure of whatever is being sent now.
    pub async fn pump(&mut self) -> Result<(), ClientError> {
        loop {
            // A zero timeout reads what has already arrived and waits
            // for nothing: the frames that are not here yet are read
            // when the client asks for the next event.
            let message =
                match tokio::time::timeout(std::time::Duration::ZERO, self.socket.next()).await {
                    Ok(Some(Ok(message))) => message,
                    _ => return Ok(()),
                };
            let Message::Text(text) = message else {
                continue;
            };
            if let Some(incoming) = self.take(&text)? {
                self.pending.push_back(incoming);
            }
        }
    }

    /// Take what a frame has to say.
    fn take(&mut self, text: &str) -> Result<Option<Incoming>, ClientError> {
        let frame: Frame = match serde_json::from_str(text) {
            Ok(frame) => frame,
            Err(_) => return Ok(None),
        };
        match frame.kind {
            FrameType::Hello => {
                // The hello of the extension carries its public key.
                if let Some(payload) = &frame.payload {
                    self.end.meet(payload).map_err(|_| ClientError::Malformed)?;
                }
                Ok(None)
            }
            FrameType::Publish | FrameType::Result => {
                let payload = frame
                    .payload
                    .as_ref()
                    .and_then(|body| body.as_str())
                    .unwrap_or_default()
                    .to_string();
                let opened = self
                    .end
                    .open(&payload, frame.seq)
                    .map_err(|_| ClientError::Malformed)?;
                Ok(Some(Incoming::Event(Event {
                    schema: frame.schema,
                    channel: frame.channel,
                    seq: frame.seq,
                    payload: String::from_utf8_lossy(&opened).to_string(),
                    sealed: payload,
                })))
            }
            FrameType::Error => Ok(Some(Incoming::Refused(text.to_string()))),
            _ => Ok(None),
        }
    }

    /// Publish an event to the extension.
    ///
    /// The payload is sealed under the key of the tunnel; the broker
    /// carries it without opening it.
    pub async fn publish(
        &mut self,
        schema: &SchemaId,
        channel: &str,
        payload: &[u8],
    ) -> Result<(), ClientError> {
        self.pump().await?;
        let (seq, sealed) = self.end.seal(payload).map_err(|_| ClientError::NoKeys)?;
        let frame = Frame {
            kind: FrameType::Publish,
            schema: schema.clone(),
            channel: channel.to_string(),
            call: None,
            seq,
            payload: Some(serde_json::Value::String(sealed)),
        };
        self.send(&frame).await
    }

    /// Send a frame.
    async fn send(&mut self, frame: &Frame) -> Result<(), ClientError> {
        let text = serde_json::to_string(frame).map_err(ClientError::Encoding)?;
        self.socket
            .send(Message::Text(text))
            .await
            .map_err(|error| ClientError::Connection(Box::new(error)))
    }
}

/// A subscription: a channel and the scopes it was granted.
#[derive(Debug, Clone)]
pub struct Subscription {
    /// The channel.
    pub channel: String,
    /// The scopes.
    pub scopes: Vec<Scope>,
}

/// What went wrong on the client side.
#[derive(Debug)]
pub enum ClientError {
    /// The connection failed, or broke.
    Connection(Box<tokio_tungstenite::tungstenite::Error>),
    /// The broker refused something.
    Refused(String),
    /// A frame could not be encoded.
    Encoding(serde_json::Error),
    /// The address names another surface than the connection does.
    WrongSurface { wanted: String, connected: String },
    /// The extension's hello has not arrived, so there is no key.
    NoKeys,
    /// A frame was not what it should have been.
    Malformed,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Connection(error) => {
                write!(formatter, "connection failed: {error}")
            }
            ClientError::Refused(text) => write!(formatter, "broker refused: {text}"),
            ClientError::Encoding(error) => write!(formatter, "frame did not encode: {error}"),
            ClientError::WrongSurface { wanted, connected } => write!(
                formatter,
                "address names surface {wanted}; connection is on {connected}"
            ),
            ClientError::NoKeys => {
                formatter.write_str("no key yet => extension hello has not arrived")
            }
            ClientError::Malformed => formatter.write_str("malformed frame"),
        }
    }
}

impl std::error::Error for ClientError {}

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
