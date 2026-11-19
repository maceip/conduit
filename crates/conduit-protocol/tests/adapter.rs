//! The tests of the adapter contract.
//!
//! The worked example is a file adapter: it reads a real file from
//! disk, and when the file is not there it says nothing.  That is the
//! behaviour every adapter must have — the state is real or the
//! publish is empty; there is no third answer.

use async_trait::async_trait;
use conduit_protocol::adapter::{mail, SurfaceAdapter};
use conduit_protocol::{verb, SchemaId, Scope, Surface};
use std::path::PathBuf;

/// A worked example: a surface whose state is the contents of a file.
struct FileAdapter {
    path: PathBuf,
}

#[async_trait]
impl SurfaceAdapter for FileAdapter {
    fn surface(&self) -> Surface {
        Surface {
            id: SchemaId::new("dev.example.file", 1),
            schemas: vec![SchemaId::new("dev.example.file.contents", 1)],
            scopes: vec![Scope::new("file", verb::READ)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        }
    }

    async fn read(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }
}

/// The fixture file of a test: on disk, and gone with the test.
struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn with(contents: &str) -> Fixture {
        let path = std::env::temp_dir().join(format!(
            "conduit-adapter-{}-{}",
            std::process::id(),
            contents.len()
        ));
        std::fs::write(&path, contents).expect("the fixture file writes");
        Fixture { path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[tokio::test]
async fn the_example_adapter_reads_real_state() {
    let fixture = Fixture::with("the state is real\n");
    let adapter = FileAdapter {
        path: fixture.path.clone(),
    };
    assert_eq!(adapter.read().await.as_deref(), Some("the state is real\n"));
}

#[tokio::test]
async fn an_adapter_with_no_state_publishes_nothing() {
    let adapter = FileAdapter {
        path: PathBuf::from("/nonexistent/conduit/adapter/fixture"),
    };
    assert!(adapter.read().await.is_none());
}

#[tokio::test]
async fn the_example_adapter_names_what_it_offers() {
    let adapter = FileAdapter {
        path: PathBuf::from("/nonexistent/conduit/adapter/fixture"),
    };
    let surface = adapter.surface();
    assert_eq!(surface.id, SchemaId::new("dev.example.file", 1));
    assert!(surface.offers(&Scope::new("file", verb::READ)));
    assert!(surface.carries(&SchemaId::new("dev.example.file.contents", 1)));
}

#[test]
fn the_mail_surface_family_is_the_four_kinds_the_research_names() {
    let kinds = [
        (mail::compose_surface(), "dev.conduit.mail.compose"),
        (mail::action_surface(), "dev.conduit.mail.action"),
        (
            mail::content_blocker_surface(),
            "dev.conduit.mail.contentblocker",
        ),
        (
            mail::message_security_surface(),
            "dev.conduit.mail.security",
        ),
    ];
    for (surface, id) in kinds {
        assert_eq!(surface.id, SchemaId::new(id, 1));
        assert!(!surface.scopes.is_empty(), "{id} declares its scopes");
        assert!(!surface.schemas.is_empty(), "{id} declares its schemas");
    }

    /* The kinds do not overlap: one kind cannot speak for another. */
    let compose = mail::compose_surface();
    let action = mail::action_surface();
    assert!(!compose.offers(&Scope::new("mail.action", verb::WRITE)));
    assert!(!action.offers(&Scope::new("mail.compose", verb::WRITE)));
}
