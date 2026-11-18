//! One frame from the tab surface.
//!
//! The far end of the scripted browser run: it subscribes to the tab
//! surface of the shipped extension, asks for the tab, and prints the
//! first event it receives.  The event's payload is opened from the
//! sealed frame the extension published; the broker never saw it.
//!
//! Run it against a local broker and a connected extension:
//!
//! ```sh
//! cargo run -p conduit-client --example tab_frame
//! ```

use conduit_client::{Address, Client};
use conduit_protocol::{verb, SchemaId, Scope};
use std::time::Duration;

#[tokio::main]
async fn main() {
    let broker = std::env::var("CONDUIT_BROKER")
        .unwrap_or_else(|_| "ws://127.0.0.1:8787/v1/tunnel".to_string());
    let token = std::env::var("CONDUIT_TOKEN").unwrap_or_else(|_| "bootstrap-token".to_string());
    let address = Address::parse("conduit://dev.conduit.browser.tabs/tabs@v1")
        .expect("the address is an address");

    let mut client = Client::connect(&broker, &token, &address)
        .await
        .expect("the broker is listening");
    client
        .subscribe(&address, &[Scope::new("browser.tabs", verb::READ)])
        .await
        .expect("the scope is granted");

    /* Ask for the tab: a sealed frame to the extension, which answers
     * with what the surface reads. */
    client
        .publish(&SchemaId::new("dev.conduit.browser.tab", 1), "tabs", b"{}")
        .await
        .expect("the request goes");

    let event = tokio::time::timeout(Duration::from_secs(30), client.next())
        .await
        .expect("the frame arrives before the timeout")
        .expect("the tunnel stays open")
        .expect("the tunnel stays open");

    println!("{}", event.payload);
}
