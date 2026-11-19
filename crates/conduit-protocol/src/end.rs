//! One end of a tunnel: the keys, the hello, the sealing.
//!
//! Both ends of a tunnel do the same three things: they say hello with
//! their public key and the prefix of their nonces, they agree the
//! keys of the tunnel with the hello they get back, and they seal what
//! they send and open what they receive.  That is this module, and
//! everything that carries a tunnel uses it: the browser's near end
//! does the same thing in JavaScript, and the two are checked against
//! each other on a shared vector.
//!
//! The two directions get different keys, so a frame sent by the
//! extension cannot be replayed as a frame sent by the client.  The
//! names say which is which: `to_extension` is the key that seals what
//! the **extension** sends, `to_client` the one that seals what the
//! **client** sends.

use crate::sealed::{self, KeyPair, SharedSecret};
use crate::{Frame, FrameType, SchemaId};
use serde_json::Value;

/// Which end of the tunnel this is.
///
/// The role decides which of the two directional keys this end seals
/// with, and which one it opens with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The end inside the app: it publishes what the app knows.
    Extension,
    /// The end outside: it receives what the app published.
    Client,
}

/// Why an end could not do what it was asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndError {
    /// The frame was not what it should have been.
    Malformed,
    /// The other end has not said hello yet, so there is no key.
    NoKeys,
}

impl std::fmt::Display for EndError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EndError::Malformed => formatter.write_str("malformed frame"),
            EndError::NoKeys => {
                formatter.write_str("no key yet => the other end has not said hello")
            }
        }
    }
}

impl std::error::Error for EndError {}

/// One end of a tunnel.
pub struct End {
    /// Which end this is.
    role: Role,
    /// The key pair of this end.  The secret half never leaves.
    keys: KeyPair,
    /// The prefix of this end's nonces.
    prefix: [u8; 4],
    /// The prefix of the other end's nonces, once it has said hello.
    peer_prefix: Option<[u8; 4]>,
    /// The shared secret, once the two ends have met.
    shared: Option<SharedSecret>,
    /// The sequence number of the last frame this end sent.
    sequence: u64,
}

impl End {
    /// A new end, with its own keys and its own nonce prefix.
    pub fn new(role: Role) -> End {
        End {
            role,
            keys: KeyPair::generate(),
            prefix: sealed::random_prefix(),
            peer_prefix: None,
            shared: None,
            sequence: 0,
        }
    }

    /// The public key of this end, base64url without padding.
    pub fn public(&self) -> String {
        sealed::to_wire(&self.keys.public_bytes())
    }

    /// The prefix of this end's nonces, base64url without padding.
    pub fn prefix(&self) -> String {
        sealed::to_wire(&self.prefix)
    }

    /// The sequence number of the last frame this end sent.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// The next sequence number of this end.
    ///
    /// The broker keeps the number of the end that sent the frame,
    /// because that number is part of the nonce the payload is sealed
    /// under: a frame the broker renumbered could not be opened.
    pub fn next_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }

    /// The hello, which says who this end is and where it got to.
    pub fn hello(&mut self, surface: &str, channel: &str) -> Frame {
        let sequence = self.next_sequence();
        Frame {
            kind: FrameType::Hello,
            schema: SchemaId::new("dev.conduit.hello", 1),
            channel: channel.to_string(),
            call: None,
            seq: sequence,
            payload: Some(serde_json::json!({
                "role": match self.role {
                    Role::Extension => "extension",
                    Role::Client => "client",
                },
                "surface": surface,
                "resume": self.sequence,
                "crypto": {
                    "scheme": sealed::SCHEME,
                    "public": self.public(),
                    "prefix": self.prefix(),
                },
            })),
        }
    }

    /// Take the hello of the other end and agree the keys of the tunnel.
    pub fn meet(&mut self, payload: &Value) -> Result<(), EndError> {
        let value = payload;
        let crypto = value.get("crypto").ok_or(EndError::Malformed)?;
        let public = crypto
            .get("public")
            .and_then(|public| public.as_str())
            .ok_or(EndError::Malformed)?;
        let prefix = crypto
            .get("prefix")
            .and_then(|prefix| prefix.as_str())
            .ok_or(EndError::Malformed)?;

        let public = sealed::from_wire(public).ok_or(EndError::Malformed)?;
        if public.len() != 65 {
            return Err(EndError::Malformed);
        }
        let mut point = [0u8; 65];
        point.copy_from_slice(&public);
        let shared = self.keys.agree(&point);

        let prefix = sealed::from_wire(prefix).ok_or(EndError::Malformed)?;
        if prefix.len() != 4 {
            return Err(EndError::Malformed);
        }
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&prefix);

        self.peer_prefix = Some(bytes);
        self.shared = Some(shared);
        Ok(())
    }

    /// Whether the two ends have met.
    pub fn agreed(&self) -> bool {
        self.shared.is_some()
    }

    /// Seal a payload for the other end.
    ///
    /// The nonce is four random bytes and the frame's sequence number,
    /// and the sequence number is also the additional data: the order
    /// is authenticated as well as the content.
    pub fn seal(&mut self, payload: &[u8]) -> Result<(u64, String), EndError> {
        let shared = self.shared.as_ref().ok_or(EndError::NoKeys)?;
        let key = match self.role {
            Role::Extension => shared.to_extension(),
            Role::Client => shared.to_client(),
        };
        let sequence = self.next_sequence();
        let sealed = sealed::seal(&key, self.prefix, sequence, payload);
        Ok((sequence, sealed.to_base64()))
    }

    /// Open a payload from the other end.
    pub fn open(&self, payload: &str, sequence: u64) -> Result<Vec<u8>, EndError> {
        let shared = self.shared.as_ref().ok_or(EndError::NoKeys)?;
        let prefix = self.peer_prefix.as_ref().ok_or(EndError::NoKeys)?;
        let key = match self.role {
            Role::Extension => shared.to_client(),
            Role::Client => shared.to_extension(),
        };
        let blob = sealed::Sealed::from_base64(payload).ok_or(EndError::Malformed)?;
        sealed::open(&key, *prefix, sequence, &blob).ok_or(EndError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_ends_agree_and_seal_each_way() {
        let mut extension = End::new(Role::Extension);
        let mut client = End::new(Role::Client);

        let hello = extension.hello("dev.conduit.browser.tabs/v1", "tabs");
        let payload = hello.payload.clone().expect("the hello carries a payload");
        client
            .meet(&payload)
            .expect("the client meets the extension");

        let hello = client.hello("dev.conduit.browser.tabs/v1", "tabs");
        let payload = hello.payload.clone().expect("the hello carries a payload");
        extension
            .meet(&payload)
            .expect("the extension meets the client");

        // What the extension sends, the client opens.
        let (sequence, blob) = extension.seal(b"the tab").expect("the seal");
        assert_eq!(client.open(&blob, sequence).expect("the open"), b"the tab");

        // And the other way.
        let (sequence, blob) = client.seal(b"from the client").expect("the seal");
        assert_eq!(
            extension.open(&blob, sequence).expect("the open"),
            b"from the client"
        );
    }

    #[test]
    fn a_frame_does_not_open_the_other_way_round() {
        let mut extension = End::new(Role::Extension);
        let mut client = End::new(Role::Client);
        let hello = extension.hello("dev.conduit.browser.tabs/v1", "tabs");
        let payload = hello.payload.expect("the hello carries a payload");
        client
            .meet(&payload)
            .expect("the client meets the extension");
        let hello = client.hello("dev.conduit.browser.tabs/v1", "tabs");
        let payload = hello.payload.expect("the hello carries a payload");
        extension
            .meet(&payload)
            .expect("the extension meets the client");

        let (sequence, blob) = extension.seal(b"the tab").expect("the seal");
        assert!(client.open(&blob, sequence).is_ok());
        // The client's own key does not open the extension's frame.
        let (_, other) = client.seal(b"the tab").expect("the seal");
        assert!(client.open(&other, sequence).is_err());
    }

    #[test]
    fn the_hello_carries_the_key_material_base64url() {
        let mut end = End::new(Role::Extension);
        let hello = end.hello("dev.conduit.browser.tabs/v1", "tabs");
        let payload = hello.payload.expect("the hello carries a payload");
        let value: Value = payload.clone();
        let crypto = value.get("crypto").expect("the crypto");
        let public = crypto
            .get("public")
            .and_then(|v| v.as_str())
            .expect("the key");
        let prefix = crypto
            .get("prefix")
            .and_then(|v| v.as_str())
            .expect("the prefix");
        assert!(public
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        assert_eq!(sealed::from_wire(public).expect("base64url").len(), 65);
        assert_eq!(sealed::from_wire(prefix).expect("base64url").len(), 4);
    }

    #[test]
    fn a_seal_before_the_hello_is_refused() {
        let mut end = End::new(Role::Extension);
        assert_eq!(end.seal(b"too soon"), Err(EndError::NoKeys));
    }
}
