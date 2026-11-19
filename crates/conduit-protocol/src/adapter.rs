//! The adapter contract: what a near end implements to read an app.
//!
//! A surface is carried over a tunnel by machinery that knows nothing
//! about what it carries.  The adapter is the part that does: it names
//! the surface it offers and reads one app's state.  Everything the
//! research says about extension surfaces — that the near end lives
//! inside the app's own sandbox, that it decides what to publish, and
//! that a surface without state must not invent any — is this trait.
//!
//! An adapter that cannot read real state returns `None` and publishes
//! nothing.  There is no placeholder payload in this contract.

use crate::{SchemaId, Scope, Surface};
use async_trait::async_trait;

/// One surface, read from an app.
#[async_trait]
pub trait SurfaceAdapter {
    /// What this adapter offers: the surface's identity, schemas and
    /// scopes.
    fn surface(&self) -> Surface;

    /// The state the surface publishes, as a JSON payload, or `None`
    /// when there is nothing to say.
    async fn read(&self) -> Option<String>;
}

/// The mail surface family, as the research names it.
///
/// A MailKit extension comes in four kinds ([MailKit], [WWDC21 10168]):
///
/// * **compose** — validate recipients, add headers, supply a view;
/// * **action** — flag, colour, move or archive a message as it arrives;
/// * **content blocker** — WebKit rules for the message view;
/// * **message security** — sign, encrypt, decrypt.
///
/// Each kind is one surface here.  No adapter in this repository reads
/// Mail: these are the shapes a Mail adapter would implement, and they
/// say so by existing as declarations and nothing more.
///
/// [MailKit]: https://www.developer.apple.com/documentation/mailkit
/// [WWDC21 10168]: https://developer.apple.com/videos/play/wwdc2021/10168/
pub mod mail {
    use super::*;

    /// The compose kind: the message being written.
    pub fn compose_surface() -> Surface {
        Surface {
            id: SchemaId::new("dev.conduit.mail.compose", 1),
            schemas: vec![SchemaId::new("dev.conduit.mail.message", 1)],
            scopes: vec![
                Scope::new("mail.compose", crate::verb::READ),
                Scope::new("mail.compose", crate::verb::WRITE),
            ],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        }
    }

    /// The action kind: what arrives and what the user does to it.
    pub fn action_surface() -> Surface {
        Surface {
            id: SchemaId::new("dev.conduit.mail.action", 1),
            schemas: vec![SchemaId::new("dev.conduit.mail.message", 1)],
            scopes: vec![Scope::new("mail.action", crate::verb::WRITE)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        }
    }

    /// The content blocker kind: the rules for the message view.
    pub fn content_blocker_surface() -> Surface {
        Surface {
            id: SchemaId::new("dev.conduit.mail.contentblocker", 1),
            schemas: vec![SchemaId::new("dev.conduit.mail.rules", 1)],
            scopes: vec![Scope::new("mail.contentblocker", crate::verb::WRITE)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        }
    }

    /// The message security kind: sign, encrypt, decrypt.
    pub fn message_security_surface() -> Surface {
        Surface {
            id: SchemaId::new("dev.conduit.mail.security", 1),
            schemas: vec![SchemaId::new("dev.conduit.mail.message", 1)],
            scopes: vec![
                Scope::new("mail.message", crate::verb::READ),
                Scope::new("mail.message", crate::verb::WRITE),
            ],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        }
    }
}
