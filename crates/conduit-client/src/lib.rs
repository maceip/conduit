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

use conduit_protocol::{Frame, FrameType, SchemaId, Scope};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
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
            AddressError::NoScheme => formatter.write_str("the address has no scheme"),
            AddressError::NoAuthority => formatter.write_str("the address has no authority"),
            AddressError::NoVersion => formatter.write_str("the address has no version"),
        }
    }
}

impl std::error::Error for AddressError {}

/// An event: a frame, with its payload still sealed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// The schema the payload follows.
    pub schema: SchemaId,
    /// The channel the event belongs to.
    pub channel: String,
    /// The sequence number of the event.
    pub seq: u64,
    /// The sealed payload.
    pub payload: String,
}

/// A client: a connection to the broker.
pub struct Client {
    socket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    sequence: u64,
    channels: HashMap<String, Vec<Scope>>,
    surface: SchemaId,
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
            sequence: 0,
            channels: HashMap::new(),
            surface: address.surface(),
        };
        let hello = Frame {
            kind: FrameType::Hello,
            schema: SchemaId::new("dev.conduit.hello", 1),
            channel: "tunnel".to_string(),
            call: None,
            seq: 0,
            payload: None,
        };
        client.send(&hello).await?;
        Ok(client)
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
        let channel = address.channel();
        let payload = serde_json::json!({
            "channel": channel,
            "scopes": scopes.iter().map(|scope| scope.to_string()).collect::<Vec<_>>(),
        })
        .to_string();
        let frame = Frame {
            kind: FrameType::Subscribe,
            schema: SchemaId::new("dev.conduit.subscribe", 1),
            channel: channel.clone(),
            call: None,
            seq: self.next_sequence(),
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
            let message = self.socket.next().await;
            let Some(message) = message else {
                return Ok(None);
            };
            let message = message.map_err(|error| ClientError::Connection(Box::new(error)))?;
            let Message::Text(text) = message else {
                continue;
            };
            let frame: Frame = match serde_json::from_str(&text) {
                Ok(frame) => frame,
                Err(_) => continue,
            };
            match frame.kind {
                FrameType::Publish => {
                    return Ok(Some(Event {
                        schema: frame.schema,
                        channel: frame.channel,
                        seq: frame.seq,
                        payload: frame.payload.unwrap_or_default(),
                    }));
                }
                FrameType::Error => {
                    return Err(ClientError::Refused(text.to_string()));
                }
                FrameType::Ping => {
                    let pong = Frame {
                        kind: FrameType::Pong,
                        schema: SchemaId::new("dev.conduit.pong", 1),
                        channel: "tunnel".to_string(),
                        call: None,
                        seq: self.next_sequence(),
                        payload: None,
                    };
                    self.send(&pong).await?;
                }
                _ => continue,
            }
        }
    }

    /// Send a frame.
    async fn send(&mut self, frame: &Frame) -> Result<(), ClientError> {
        let text = serde_json::to_string(frame).map_err(ClientError::Encoding)?;
        self.socket
            .send(Message::Text(text))
            .await
            .map_err(|error| ClientError::Connection(Box::new(error)))
    }

    /// The next sequence number of the client.
    fn next_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
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
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Connection(error) => {
                write!(formatter, "the connection failed: {error}")
            }
            ClientError::Refused(text) => write!(formatter, "the broker refused: {text}"),
            ClientError::Encoding(error) => write!(formatter, "the frame did not encode: {error}"),
            ClientError::WrongSurface { wanted, connected } => write!(
                formatter,
                "the address names the surface {wanted} and the connection is {connected}"
            ),
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
