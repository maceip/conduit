//! The frames, the scopes, the schemes.
//!
//! Everything that both ends of a tunnel have to agree on lives here:
//! the envelope, the registry of frame types, the identifiers of schemas
//! and surfaces, the scopes, and the errors.
//!
//! The JSON Schemas in `spec/schemas/` are the normative form of all of
//! it.  This crate is the same thing in Rust, and it checks itself
//! against them in the tests that live in `tests/`.

use serde::{Deserialize, Serialize};
use std::fmt;

pub mod adapter;
pub mod sealed;

/// The version of the protocol this crate speaks.
pub const PROTOCOL_VERSION: u16 = 1;

/// The largest sealed payload a frame may carry.
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

/// The number of frames the broker keeps per tunnel, for resume.
pub const REPLAY_RING: usize = 256;

/// The frame types that are registered.
///
/// An unknown type is refused, not ignored: a peer that speaks a type we
/// do not know is a peer whose intent we cannot guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameType {
    Hello,
    Welcome,
    Subscribe,
    Subscribed,
    Unsubscribe,
    Publish,
    Call,
    Result,
    Error,
    Ping,
    Pong,
    Grant,
}

impl FrameType {
    /// Every registered type, in the order of the registry.
    pub const ALL: [FrameType; 12] = [
        FrameType::Hello,
        FrameType::Welcome,
        FrameType::Subscribe,
        FrameType::Subscribed,
        FrameType::Unsubscribe,
        FrameType::Publish,
        FrameType::Call,
        FrameType::Result,
        FrameType::Error,
        FrameType::Ping,
        FrameType::Pong,
        FrameType::Grant,
    ];

    /// The name of the type on the wire.
    pub fn name(&self) -> &'static str {
        match self {
            FrameType::Hello => "hello",
            FrameType::Welcome => "welcome",
            FrameType::Subscribe => "subscribe",
            FrameType::Subscribed => "subscribed",
            FrameType::Unsubscribe => "unsubscribe",
            FrameType::Publish => "publish",
            FrameType::Call => "call",
            FrameType::Result => "result",
            FrameType::Error => "error",
            FrameType::Ping => "ping",
            FrameType::Pong => "pong",
            FrameType::Grant => "grant",
        }
    }

    /// The type with that name, if it is registered.
    pub fn from_name(name: &str) -> Option<FrameType> {
        FrameType::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A frame, as it is on the wire.
///
/// The envelope carries the type of the frame, the schema of its payload,
/// the channel it belongs to, the call it answers, the sequence number of
/// the tunnel, and the payload itself — sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// The type of the frame.
    #[serde(rename = "t")]
    pub kind: FrameType,
    /// The schema id of the payload.
    #[serde(rename = "s")]
    pub schema: SchemaId,
    /// The channel the frame belongs to.
    #[serde(rename = "c")]
    pub channel: String,
    /// The call this frame answers, if any.
    #[serde(rename = "i", skip_serializing_if = "Option::is_none", default)]
    pub call: Option<u64>,
    /// The sequence number of this frame in the tunnel.
    #[serde(rename = "seq")]
    pub seq: u64,
    /// The sealed payload.
    #[serde(rename = "p", skip_serializing_if = "Option::is_none", default)]
    pub payload: Option<String>,
}

impl Frame {
    /// A frame of that type, on that channel, with that payload.
    pub fn new(kind: FrameType, channel: &str, payload: Option<String>) -> Frame {
        Frame {
            kind,
            schema: SchemaId::new("dev.conduit.frame", 1),
            channel: channel.to_string(),
            call: None,
            seq: 0,
            payload,
        }
    }

    /// Whether the frame is within the size limit.
    pub fn within_the_limit(&self) -> bool {
        self.payload.as_ref().map(|body| body.len()).unwrap_or(0) <= MAX_FRAME_BYTES
    }
}

/// The identifier of a schema: reverse DNS with a version.
///
/// The shape is the one MCP uses for its extensions
/// (`io.modelcontextprotocol/ui`), with the version carried in the
/// identifier rather than in a separate field, so that a schema id names
/// one thing and only one thing.
///
/// On the wire it is a string — `dev.conduit.mail.message/v1` — and not a
/// pair of fields, so that a reader can compare two identifiers without
/// knowing their shape first.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SchemaId {
    name: String,
    major: u16,
}

impl SchemaId {
    /// A schema id with that name and that version.
    pub fn new(name: &str, major: u16) -> SchemaId {
        SchemaId {
            name: name.to_string(),
            major,
        }
    }

    /// The schema id in that string, if it is one.
    ///
    /// The name is reverse DNS — lower case, dots, dashes — and the
    /// version is `v` and a number.
    pub fn parse(text: &str) -> Option<SchemaId> {
        let (name, version) = text.rsplit_once('/')?;
        let major = version.strip_prefix('v')?.parse::<u16>().ok()?;
        if !name_is_well_formed(name) {
            return None;
        }
        Some(SchemaId::new(name, major))
    }

    /// The name of the schema, without its version.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The version of the schema.
    pub fn major(&self) -> u16 {
        self.major
    }
}

impl fmt::Display for SchemaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/v{}", self.name, self.major)
    }
}

impl Serialize for SchemaId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for SchemaId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<SchemaId, D::Error> {
        let text = String::deserialize(deserializer)?;
        SchemaId::parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("not a schema id: {text}")))
    }
}

/// Whether a name is reverse DNS: lower case, dots and dashes only.
fn name_is_well_formed(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            !part.is_empty()
                && part.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
        })
}

/// A scope: a namespace and a verb.
///
/// The namespace belongs to the surface that defines it, so a scope
/// cannot be forged by another surface and a client cannot widen a grant
/// by asking for the same scope twice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Scope {
    namespace: String,
    verb: String,
}

impl Scope {
    /// A scope with that namespace and that verb.
    pub fn new(namespace: &str, verb: &str) -> Scope {
        Scope {
            namespace: namespace.to_string(),
            verb: verb.to_string(),
        }
    }

    /// The namespace of the scope.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// The verb of the scope.
    pub fn verb(&self) -> &str {
        &self.verb
    }

    /// Whether the scope exercises that verb.
    pub fn is(&self, verb: &str) -> bool {
        self.verb == verb
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.namespace, self.verb)
    }
}

/// The verbs a scope may have.
pub mod verb {
    /// The extension publishes, the client receives.
    pub const READ: &str = "read";
    /// The client publishes, the extension receives.
    pub const WRITE: &str = "write";
    /// The client may invoke calls on the surface.
    pub const CALL: &str = "call";
    /// The client may change the surface's own settings.
    pub const ADMIN: &str = "admin";
}

/// A surface: a named capability an extension offers.
///
/// A surface has an identifier, a set of schemas, and a set of scopes.
/// The user grants a subset of the scopes; the broker holds the grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Surface {
    /// The identifier of the surface: reverse DNS with a version.
    pub id: SchemaId,
    /// The schemas the surface offers.
    pub schemas: Vec<SchemaId>,
    /// The scopes the surface offers.
    pub scopes: Vec<Scope>,
    /// The schemes the surface may be reached over.
    pub schemes: Vec<String>,
}

impl Surface {
    /// Whether the surface offers that scope.
    pub fn offers(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }

    /// Whether the surface offers that schema.
    pub fn carries(&self, schema: &SchemaId) -> bool {
        self.schemas.contains(schema)
    }
}

/// The user's decision about a surface.
///
/// This is the thing the whole design turns on: the broker holds it, and
/// it is the only thing that says what may cross the tunnel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grant {
    /// The surface the grant is about.
    pub surface: SchemaId,
    /// The instance of the extension the grant is about.
    pub instance: String,
    /// The scopes that were granted.
    pub scopes: Vec<Scope>,
    /// The schemes that were granted.
    pub schemes: Vec<String>,
}

impl Grant {
    /// Whether the grant covers that scope.
    pub fn covers(&self, scope: &Scope) -> bool {
        self.scopes.contains(scope)
    }

    /// The scopes of the grant that are not in that list.
    ///
    /// The client asks for what it wants; this is what the broker refuses.
    pub fn missing(&self, wanted: &[Scope]) -> Vec<Scope> {
        wanted
            .iter()
            .filter(|scope| !self.covers(scope))
            .cloned()
            .collect()
    }
}

/// The errors of the protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Unauthorized,
    ScopeDenied,
    UnknownType,
    SchemaInvalid,
    SchemaUnknown,
    SurfaceUnknown,
    Expired,
    TooLarge,
}

impl ErrorCode {
    /// The name of the error on the wire.
    pub fn name(&self) -> &'static str {
        match self {
            ErrorCode::Unauthorized => "unauthorized",
            ErrorCode::ScopeDenied => "scope_denied",
            ErrorCode::UnknownType => "unknown_type",
            ErrorCode::SchemaInvalid => "schema_invalid",
            ErrorCode::SchemaUnknown => "schema_unknown",
            ErrorCode::SurfaceUnknown => "surface_unknown",
            ErrorCode::Expired => "expired",
            ErrorCode::TooLarge => "too_large",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// The error a frame may carry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    /// The code of the error.
    pub code: ErrorCode,
    /// A human-readable message.
    pub message: String,
    /// The id of the frame the error answers.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reference: Option<u64>,
    /// The scope that was denied, if the error is about a scope.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<String>,
}

/// What went wrong on the wire.
#[derive(Debug)]
pub enum Failure {
    /// The frame was not valid JSON, or did not match the envelope.
    Malformed(String),
    /// The frame type is not registered.
    UnknownType(String),
    /// The frame was over the size limit.
    TooLarge,
    /// The scope named by the frame is not covered by the grant.
    ScopeDenied(Scope),
    /// There is no such surface.
    UnknownSurface(SchemaId),
    /// The credential was not good.
    Unauthorized,
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Malformed(detail) => write!(formatter, "malformed frame: {detail}"),
            Failure::UnknownType(name) => write!(formatter, "unknown frame type: {name}"),
            Failure::TooLarge => write!(formatter, "frame is too large"),
            Failure::ScopeDenied(scope) => write!(formatter, "scope not granted: {scope}"),
            Failure::UnknownSurface(id) => write!(formatter, "unknown surface: {id}"),
            Failure::Unauthorized => write!(formatter, "not authorized"),
        }
    }
}

impl std::error::Error for Failure {}

/// The failure of a frame, with the code the wire carries.
impl Failure {
    /// The code that goes with that failure.
    pub fn code(&self) -> ErrorCode {
        match self {
            Failure::Malformed(_) => ErrorCode::SchemaInvalid,
            Failure::UnknownType(_) => ErrorCode::UnknownType,
            Failure::TooLarge => ErrorCode::TooLarge,
            Failure::ScopeDenied(_) => ErrorCode::ScopeDenied,
            Failure::UnknownSurface(_) => ErrorCode::SurfaceUnknown,
            Failure::Unauthorized => ErrorCode::Unauthorized,
        }
    }

    /// The scope the failure is about, if it is about one.
    pub fn scope(&self) -> Option<&Scope> {
        match self {
            Failure::ScopeDenied(scope) => Some(scope),
            _ => None,
        }
    }
}

/// The frames the broker keeps, for resume.
#[derive(Debug, Default)]
pub struct Replay {
    frames: Vec<(u64, Frame)>,
}

impl Replay {
    /// A ring that keeps the last `REPLAY_RING` frames.
    pub fn new() -> Replay {
        Replay::default()
    }

    /// Put a frame into the ring.
    pub fn push(&mut self, seq: u64, frame: Frame) {
        self.frames.push((seq, frame));
        if self.frames.len() > REPLAY_RING {
            let excess = self.frames.len() - REPLAY_RING;
            self.frames.drain(0..excess);
        }
    }

    /// The frames after that sequence number, and the sequence number the
    /// replay stopped at.
    ///
    /// The second part of the answer is whether the gap was larger than
    /// the ring: when it was, the end must take a fresh snapshot, because
    /// it has missed frames it cannot have.
    pub fn replay_from(&self, after: u64) -> (Vec<Frame>, u64, bool) {
        let first = self.frames.first().map(|(seq, _)| *seq).unwrap_or(0);
        let gap = !self.frames.is_empty() && after + 1 < first;
        let frames = self
            .frames
            .iter()
            .filter(|(seq, _)| *seq > after)
            .map(|(_, frame)| frame.clone())
            .collect();
        let stopped = self.frames.last().map(|(seq, _)| *seq).unwrap_or(after);
        (frames, stopped, gap)
    }

    /// How many frames the ring holds.
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Whether the ring is empty.
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}
