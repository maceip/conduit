//! The device codes: how an extension gets a secret.
//!
//! The extension cannot ask the broker for a secret out of nowhere —
//! the broker would be handing a tunnel to anyone who asked.  So the
//! extension asks for a **code**, shows it to the user, the user
//! approves it in the broker's own page, and only then does the
//! extension get a secret.
//!
//! The code is the only thing that stands between the extension and
//! the user's consent, so it is long, it is random, and it does not
//! live long.

use conduit_protocol::{Grant, SchemaId, Scope};
use rand_core::{OsRng, RngCore};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// How long a code lives.
pub const CODE_LIFETIME_SECONDS: u64 = 600;

/// The bytes of a code.
const CODE_BYTES: usize = 24;

/// The bytes of a secret.
const SECRET_BYTES: usize = 32;

/// What a code is for.
#[derive(Debug, Clone)]
pub struct Device {
    /// The surface the extension wants to offer.
    pub surface: SchemaId,
    /// The scopes the extension asks for.
    pub scopes: Vec<Scope>,
    /// Whether the user has approved it.
    pub approved: bool,
    /// The secret, once it has been given out.
    pub secret: Option<String>,
    /// When the code was made.
    pub made: std::time::Instant,
}

impl Device {
    /// Whether the code has outlived its welcome.
    pub fn expired(&self) -> bool {
        self.made.elapsed().as_secs() > CODE_LIFETIME_SECONDS
    }
}

/// The codes the broker knows.
#[derive(Default)]
pub struct Devices {
    codes: RwLock<HashMap<String, Device>>,
    /// The secrets the broker has given out, by secret.
    secrets: RwLock<HashMap<String, SchemaId>>,
}

impl Devices {
    /// An empty set of codes.
    pub fn new() -> Arc<Devices> {
        Arc::new(Devices::default())
    }

    /// A new code for that surface and those scopes.
    pub async fn ask(&self, surface: SchemaId, scopes: Vec<Scope>) -> String {
        let mut bytes = [0u8; CODE_BYTES];
        OsRng.fill_bytes(&mut bytes);
        let code = base64_url::encode(&bytes);
        self.codes.write().await.insert(
            code.clone(),
            Device {
                surface,
                scopes,
                approved: false,
                secret: None,
                made: std::time::Instant::now(),
            },
        );
        code
    }

    /// The device of that code, if it is still there.
    pub async fn device(&self, code: &str) -> Option<Device> {
        self.codes.read().await.get(code).cloned()
    }

    /// The user approves the code.
    pub async fn approve(&self, code: &str) -> Option<Device> {
        let mut codes = self.codes.write().await;
        let device = codes.get_mut(code)?;
        if device.expired() {
            return None;
        }
        device.approved = true;
        Some(device.clone())
    }

    /// The secret of an approved code, given out once.
    pub async fn secret(&self, code: &str, instance: &str) -> Option<(String, Grant)> {
        let mut codes = self.codes.write().await;
        let device = codes.get_mut(code)?;
        if device.expired() || !device.approved {
            return None;
        }
        let secret = match &device.secret {
            Some(secret) => secret.clone(),
            None => {
                let mut bytes = [0u8; SECRET_BYTES];
                OsRng.fill_bytes(&mut bytes);
                let secret = base64_url::encode(&bytes);
                device.secret = Some(secret.clone());
                self.secrets
                    .write()
                    .await
                    .insert(secret.clone(), device.surface.clone());
                secret
            }
        };
        let grant = Grant {
            surface: device.surface.clone(),
            instance: instance.to_string(),
            scopes: device.scopes.clone(),
            schemes: vec![conduit_protocol::sealed::SCHEME.to_string()],
        };
        Some((secret, grant))
    }

    /// The surface a secret speaks for.
    pub async fn surface_of(&self, secret: &str) -> Option<SchemaId> {
        self.secrets.read().await.get(secret).cloned()
    }

    /// The grant a secret speaks for.
    pub async fn grant_for(&self, secret: &str, instance: &str) -> Option<Grant> {
        let surface = self.surface_of(secret).await?;
        Some(Grant {
            surface,
            instance: instance.to_string(),
            scopes: Vec::new(),
            schemes: vec![conduit_protocol::sealed::SCHEME.to_string()],
        })
    }
}
