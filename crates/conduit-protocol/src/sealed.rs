//! The sealing: the payload between the two ends, and nobody else's.
//!
//! The scheme is `p256-aes256gcm/v1`:
//!
//!   - the two ends agree a key with ECDH over P-256, and neither the
//!     broker nor anyone else is a party to it;
//!   - the shared secret is run through HKDF-SHA256 to make **two**
//!     directional keys, so a frame cannot be replayed backwards;
//!   - the payload is sealed with AES-256-GCM under a nonce that carries
//!     the frame's sequence number, with the sequence number also as the
//!     additional data, so the ordering is authenticated as well as the
//!     content;
//!   - the sealed blob is base64url without padding, in the frame's `p`
//!     field.
//!
//! Two choices were forced by what the browser's WebCrypto can
//! actually do:
//!
//!   - **AES-GCM, not ChaCha20-Poly1305**: the browser has no
//!     ChaCha20-Poly1305.
//!   - **P-256, not X25519**: the browser's X25519 cannot take a raw
//!     private key with the usage the derivation needs, so a fixed key
//!     cannot be checked against this one.  P-256 takes a raw key in
//!     the JWK form, and P-256 is what FIPS names.
//!
//! The nonce is four random bytes and the sequence number: the sequence
//! number makes it unique per frame, and the random prefix makes it
//! unique per key agreement even if the sequence numbers restart.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::Aes256Gcm;
use aes_gcm::KeyInit;
use hkdf::Hkdf;
use p256::elliptic_curve::sec1::ToEncodedPoint;
use p256::{PublicKey, SecretKey};
use rand_core::{OsRng, RngCore};
use sha2::Sha256;

/// The scheme this module implements.
pub const SCHEME: &str = "p256-aes256gcm/v1";

/// The number of bytes of the nonce that come from the random prefix.
const NONCE_PREFIX: usize = 4;

/// The number of bytes of the nonce that carry the sequence number.
const NONCE_SUFFIX: usize = 8;

/// The number of bytes of a key.
const KEY_LEN: usize = 32;

/// The number of bytes of a nonce.
const NONCE_LEN: usize = NONCE_PREFIX + NONCE_SUFFIX;

/// One end's key pair.
#[derive(Clone)]
pub struct KeyPair {
    secret: SecretKey,
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("KeyPair")
            .field("public", &self.public_hex())
            .finish_non_exhaustive()
    }
}

impl KeyPair {
    /// A new key pair, from the operating system's randomness.
    pub fn generate() -> KeyPair {
        let mut bytes = [0u8; 32];
        OsRng.fill_bytes(&mut bytes);
        KeyPair {
            secret: SecretKey::from_bytes(&bytes.into()).expect("a random scalar is in range"),
        }
    }

    /// The key pair of that secret key.
    pub fn from_bytes(bytes: [u8; 32]) -> KeyPair {
        KeyPair {
            secret: SecretKey::from_bytes(&bytes.into()).expect("the scalar is in range"),
        }
    }

    /// The public key, as the uncompressed point of SEC 1.
    ///
    /// Sixty-five bytes: the marker, the x coordinate and the y
    /// coordinate.  This is what the browser's WebCrypto exports, and
    /// what it imports.
    pub fn public_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        let point = self.secret.public_key().to_encoded_point(false);
        bytes.copy_from_slice(point.as_bytes());
        bytes
    }

    /// The secret key, as 32 bytes.
    pub fn secret_bytes(&self) -> [u8; 32] {
        self.secret.to_bytes().into()
    }

    /// The public key.
    pub fn public(&self) -> PublicKey {
        self.secret.public_key()
    }

    /// The public key, as hex.
    pub fn public_hex(&self) -> String {
        public_key_hex(&self.public_bytes())
    }

    /// The shared secret with the other end's public key.
    ///
    /// The peer key is the uncompressed point of SEC 1, sixty-five
    /// bytes.  The result is symmetric: both ends compute the same
    /// thing, and neither end can compute it alone.
    pub fn agree(&self, peer: &[u8; 65]) -> SharedSecret {
        let peer = PublicKey::from_sec1_bytes(peer).expect("the point is on the curve");
        let shared = p256::ecdh::diffie_hellman(self.secret.to_nonzero_scalar(), peer.as_affine());
        SharedSecret::from_shared(shared.raw_secret_bytes().as_slice())
    }
}

/// The secret both ends share.
#[derive(Clone)]
pub struct SharedSecret {
    bytes: [u8; 32],
}

impl std::fmt::Debug for SharedSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SharedSecret(..)")
    }
}

impl SharedSecret {
    /// The shared secret of that agreement.
    pub fn from_shared(shared: &[u8]) -> SharedSecret {
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(shared);
        SharedSecret { bytes }
    }

    /// The key for one direction.
    ///
    /// The two directions get different keys, so a frame sent by the
    /// extension cannot be replayed as a frame sent by the client.
    ///
    /// The salt is the string of zeros RFC 5869 names, written out
    /// rather than left to the library's default, so that the browser's
    /// implementation of HKDF agrees with this one.
    fn key(&self, direction: &[u8]) -> [u8; KEY_LEN] {
        let hkdf = Hkdf::<Sha256>::new(Some(&[0u8; 32]), &self.bytes);
        let mut key = [0u8; KEY_LEN];
        hkdf.expand(direction, &mut key)
            .expect("32 bytes is a valid HKDF length");
        key
    }

    /// The key that seals what the extension sends.
    pub fn to_extension(&self) -> [u8; KEY_LEN] {
        self.key(b"conduit:extension->client")
    }

    /// The key that seals what the client sends.
    pub fn to_client(&self) -> [u8; KEY_LEN] {
        self.key(b"conduit:client->extension")
    }
}

/// A sealed payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    /// The bytes of the sealed blob.
    pub bytes: Vec<u8>,
}

impl Sealed {
    /// The sealed blob, as base64url without padding.
    pub fn to_base64(&self) -> String {
        base64_url::encode(&self.bytes)
    }

    /// The sealed blob of that base64url text.
    pub fn from_base64(text: &str) -> Option<Sealed> {
        base64_url::decode(text).ok().map(|bytes| Sealed { bytes })
    }
}

/// Seal a payload under a key.
///
/// The sequence number is part of the nonce, so the frame's order is
/// authenticated with the payload itself.
pub fn seal(key: &[u8; KEY_LEN], prefix: [u8; NONCE_PREFIX], seq: u64, plaintext: &[u8]) -> Sealed {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[..NONCE_PREFIX].copy_from_slice(&prefix);
    nonce[NONCE_PREFIX..].copy_from_slice(&seq.to_be_bytes());

    let cipher = Aes256Gcm::new(key.into());
    let sealed = cipher
        .encrypt(
            nonce.as_slice().into(),
            Payload {
                msg: plaintext,
                aad: &seq.to_be_bytes(),
            },
        )
        .expect("AES-GCM accepts a message of any length");
    Sealed { bytes: sealed }
}

/// Open a sealed payload under a key.
pub fn open(
    key: &[u8; KEY_LEN],
    nonce_prefix: [u8; NONCE_PREFIX],
    seq: u64,
    sealed: &Sealed,
) -> Option<Vec<u8>> {
    let mut nonce = [0u8; NONCE_LEN];
    nonce[..NONCE_PREFIX].copy_from_slice(&nonce_prefix);
    nonce[NONCE_PREFIX..].copy_from_slice(&seq.to_be_bytes());

    let cipher = Aes256Gcm::new(key.into());
    cipher
        .decrypt(
            nonce.as_slice().into(),
            Payload {
                msg: sealed.bytes.as_slice(),
                aad: &seq.to_be_bytes(),
            },
        )
        .ok()
}

/// A random prefix for the nonce.
pub fn random_prefix() -> [u8; NONCE_PREFIX] {
    let mut prefix = [0u8; NONCE_PREFIX];
    OsRng.fill_bytes(&mut prefix);
    prefix
}

/// Bytes for the hello (a public key or a nonce prefix), as base64url
/// without padding: the same encoding as the sealed blob, and the one the
/// browser extension uses.
pub fn to_wire(bytes: &[u8]) -> String {
    base64_url::encode(bytes)
}

/// The bytes of a hello field, from base64url without padding. Anything
/// else, hex included, is refused.
pub fn from_wire(text: &str) -> Option<Vec<u8>> {
    if text.contains('=') {
        return None;
    }
    base64_url::decode(text).ok()
}

/// The public key of a secret key, as hex.
pub fn public_key_hex<const N: usize>(bytes: &[u8; N]) -> String {
    let mut text = String::with_capacity(64);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}
