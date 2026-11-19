//! The broker's binary.

use conduit_broker::{router, AppState, InMemoryRegistry};
use conduit_protocol::{Grant, SchemaId, Scope};
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    let registry = InMemoryRegistry::new();

    // The surfaces of the bootstrap: the one the shipped extension
    // offers, and the mail shape the research names.  One grant per
    // surface for each instance an end may name.
    let tabs_id = SchemaId::new("dev.conduit.browser.tabs", 1);
    registry
        .put_surface(conduit_protocol::Surface {
            id: tabs_id.clone(),
            schemas: vec![SchemaId::new("dev.conduit.browser.tab", 1)],
            scopes: vec![Scope::new("browser.tabs", conduit_protocol::verb::READ)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;

    let windows_id = SchemaId::new("dev.conduit.browser.windows", 1);
    registry
        .put_surface(conduit_protocol::Surface {
            id: windows_id.clone(),
            schemas: vec![SchemaId::new("dev.conduit.browser.window", 1)],
            scopes: vec![Scope::new("browser.windows", conduit_protocol::verb::READ)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;

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

    for instance in ["bootstrap", "extension"] {
        registry
            .put_grant(Grant {
                surface: tabs_id.clone(),
                instance: instance.to_string(),
                scopes: vec![Scope::new("browser.tabs", conduit_protocol::verb::READ)],
                schemes: vec!["p256-aes256gcm/v1".to_string()],
            })
            .await;
        registry
            .put_grant(Grant {
                surface: windows_id.clone(),
                instance: instance.to_string(),
                scopes: vec![Scope::new("browser.windows", conduit_protocol::verb::READ)],
                schemes: vec!["p256-aes256gcm/v1".to_string()],
            })
            .await;
        registry
            .put_grant(Grant {
                surface: surface_id.clone(),
                instance: instance.to_string(),
                scopes: vec![
                    Scope::new("mail.compose", conduit_protocol::verb::READ),
                    Scope::new("mail.compose", conduit_protocol::verb::WRITE),
                ],
                schemes: vec!["p256-aes256gcm/v1".to_string()],
            })
            .await;
    }
    // The token of the bootstrap: CONDUIT_CLIENT_TOKEN, or
    // `bootstrap-token`.  It covers every bootstrap surface.
    let token = std::env::var("CONDUIT_CLIENT_TOKEN")
        .ok()
        .filter(|token| !token.is_empty())
        .unwrap_or_else(|| "bootstrap-token".to_string());
    registry
        .put_token(&token, vec![tabs_id, windows_id, surface_id])
        .await;

    let state = AppState {
        registry,
        tunnels: std::sync::Arc::new(tokio::sync::Mutex::new(Default::default())),
        devices: conduit_broker::Devices::new(),
        extension_secret: "bootstrap-secret".to_string(),
    };

    // CONDUIT_LISTEN, or 127.0.0.1:8787; port 0 => a free port, printed.
    let listen = std::env::var("CONDUIT_LISTEN").unwrap_or_else(|_| "127.0.0.1:8787".to_string());
    let address: SocketAddr = listen.parse().expect("bad listen address");
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("bind failed");
    let address = listener.local_addr().expect("bound address");
    println!("conduit-broker listening on ws://{address}/v1/tunnel");
    axum::serve(listener, router(state))
        .await
        .expect("server exited");
}
