//! The registry: the surfaces, and the grants.
//!
//! The registry is the only place that knows what exists and what the
//! user allowed.  It is behind a trait so that the in-memory store of
//! the bootstrap can be replaced by a durable one without touching the
//! relay.

use async_trait::async_trait;
use conduit_protocol::{Grant, SchemaId, Surface};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// What the broker knows.
#[async_trait]
pub trait Registry: Send + Sync + 'static {
    /// The surface with that identifier.
    async fn surface(&self, id: &SchemaId) -> Option<Surface>;

    /// The grant the user gave for that surface, for that instance.
    async fn grant_for(&self, surface: &SchemaId, instance: &str) -> Grant;

    /// The surfaces the client with that token may subscribe to.
    async fn surfaces_for_token(&self, token: &str) -> Vec<SchemaId>;
}

/// The registry of the bootstrap: a map in memory.
pub struct InMemoryRegistry {
    surfaces: RwLock<HashMap<SchemaId, Surface>>,
    grants: RwLock<HashMap<(SchemaId, String), Grant>>,
    tokens: RwLock<HashMap<String, Vec<SchemaId>>>,
}

impl InMemoryRegistry {
    /// An empty registry.
    pub fn new() -> Arc<InMemoryRegistry> {
        Arc::new(InMemoryRegistry {
            surfaces: RwLock::new(HashMap::new()),
            grants: RwLock::new(HashMap::new()),
            tokens: RwLock::new(HashMap::new()),
        })
    }

    /// Put a surface in the registry.
    pub async fn put_surface(&self, surface: Surface) {
        self.surfaces
            .write()
            .await
            .insert(surface.id.clone(), surface);
    }

    /// Put a grant in the registry.
    pub async fn put_grant(&self, grant: Grant) {
        self.grants
            .write()
            .await
            .insert((grant.surface.clone(), grant.instance.clone()), grant);
    }

    /// Give a token a set of surfaces.
    pub async fn put_token(&self, token: &str, surfaces: Vec<SchemaId>) {
        self.tokens
            .write()
            .await
            .insert(token.to_string(), surfaces);
    }
}

#[async_trait]
impl Registry for InMemoryRegistry {
    async fn surface(&self, id: &SchemaId) -> Option<Surface> {
        self.surfaces.read().await.get(id).cloned()
    }

    async fn grant_for(&self, surface: &SchemaId, instance: &str) -> Grant {
        self.grants
            .read()
            .await
            .get(&(surface.clone(), instance.to_string()))
            .cloned()
            .unwrap_or_else(|| Grant {
                surface: surface.clone(),
                instance: instance.to_string(),
                scopes: Vec::new(),
                schemes: Vec::new(),
            })
    }

    async fn surfaces_for_token(&self, token: &str) -> Vec<SchemaId> {
        self.tokens
            .read()
            .await
            .get(token)
            .cloned()
            .unwrap_or_default()
    }
}
