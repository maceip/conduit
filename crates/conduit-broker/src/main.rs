//! The broker's binary.

use conduit_broker::{router, AppState, InMemoryRegistry};
use conduit_protocol::{Grant, SchemaId, Scope};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let registry = InMemoryRegistry::new();

    // The surface of the bootstrap: one surface, one grant, one token.
    let surface_id = SchemaId::new("dev.conduit.mail.compose", 1);
    registry
        .put_surface(conduit_protocol::Surface {
            id: surface_id.clone(),
            schemas: vec![SchemaId::new("dev.conduit.mail.message", 1)],
            scopes: vec![
                Scope::new("mail.compose", conduit_protocol::verb::READ),
                Scope::new("mail.compose", conduit_protocol::verb::WRITE),
            ],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;
    registry
        .put_grant(Grant {
            surface: surface_id.clone(),
            instance: "bootstrap".to_string(),
            scopes: vec![
                Scope::new("mail.compose", conduit_protocol::verb::READ),
                Scope::new("mail.compose", conduit_protocol::verb::WRITE),
            ],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;
    registry
        .put_token("bootstrap-token", vec![surface_id])
        .await;

    let state = AppState {
        registry,
        tunnels: std::sync::Arc::new(tokio::sync::Mutex::new(Default::default())),
        devices: conduit_broker::Devices::new(),
        extension_secret: "bootstrap-secret".to_string(),
    };

    let address: SocketAddr = "127.0.0.1:8787".parse().expect("the address is an address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("the port is free");
    println!("conduit-broker listening on ws://{address}/v1/tunnel");
    axum::serve(listener, router(state))
        .await
        .expect("the server runs");
}
