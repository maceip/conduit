//! The test of the device flow: the extension asks, the user approves,
//! the extension gets a secret, and the secret opens a tunnel.

use conduit_broker::{router, AppState, Devices, InMemoryRegistry};
use conduit_protocol::{Frame, FrameType, SchemaId, Scope};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

fn surface() -> SchemaId {
    SchemaId::new("dev.conduit.browser.tabs", 1)
}

async fn broker() -> String {
    let registry = InMemoryRegistry::new();
    registry
        .put_surface(conduit_protocol::Surface {
            id: surface(),
            schemas: vec![SchemaId::new("dev.conduit.browser.tab", 1)],
            scopes: vec![Scope::new("browser.tabs", conduit_protocol::verb::READ)],
            schemes: vec!["p256-aes256gcm/v1".to_string()],
        })
        .await;
    let state = AppState {
        registry,
        tunnels: Arc::new(tokio::sync::Mutex::new(Default::default())),
        devices: Devices::new(),
        extension_secret: "bootstrap-secret".to_string(),
    };
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("the address");
    tokio::spawn(async move {
        axum::serve(listener, router(state))
            .await
            .expect("the server runs");
    });
    format!("http://{address}")
}

#[tokio::test]
async fn the_user_approves_the_code_and_the_extension_gets_a_secret() {
    let address = broker().await;

    // The extension asks for a code.
    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/code"))
        .json(&serde_json::json!({
            "surface": "dev.conduit.browser.tabs/v1",
            "scopes": ["browser.tabs.read"],
        }))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(reply.status(), 200);
    let body: serde_json::Value = reply.json().await.expect("the reply is JSON");
    let code = body["code"].as_str().expect("a code").to_string();
    assert!(!code.is_empty(), "the code is not empty");
    assert_eq!(body["expires_in"], 600);

    // The secret is not there before the user approves.
    let early = reqwest::Client::new()
        .post(format!("{address}/v1/device/secret"))
        .json(&serde_json::json!({"code": code, "instance": "ext-1"}))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(
        early.status(),
        403,
        "the secret is not given out before the approval"
    );

    // The user approves.
    let approval = reqwest::Client::new()
        .post(format!("{address}/v1/device/approve"))
        .json(&serde_json::json!({"code": code}))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(approval.status(), 200);

    // The extension gets the secret.
    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/secret"))
        .json(&serde_json::json!({"code": code, "instance": "ext-1"}))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(reply.status(), 200);
    let secret = reply.text().await.expect("the secret");
    assert!(!secret.is_empty(), "the secret is not empty");

    // The grant that came with the secret is the one the user
    // approved: the surface, and the scope it asked for.
    let devices = Devices::new();
    let code = devices
        .ask(
            surface(),
            vec![Scope::new("browser.tabs", conduit_protocol::verb::READ)],
        )
        .await;
    devices.approve(&code).await.expect("the approval goes");
    let (_secret, grant) = devices.secret(&code, "ext-1").await.expect("the secret");
    assert_eq!(grant.surface, surface());
    assert_eq!(grant.scopes.len(), 1);
    assert!(grant.covers(&Scope::new("browser.tabs", conduit_protocol::verb::READ)));
}

#[tokio::test]
async fn a_code_that_was_never_asked_for_is_refused() {
    let address = broker().await;
    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/approve"))
        .json(&serde_json::json!({"code": "not-a-code"}))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(reply.status(), 404);
}

#[tokio::test]
async fn the_approval_page_names_the_surface_and_the_scopes() {
    let address = broker().await;
    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/code"))
        .json(&serde_json::json!({
            "surface": "dev.conduit.browser.tabs/v1",
            "scopes": ["browser.tabs.read"],
        }))
        .send()
        .await
        .expect("the request goes");
    let body: serde_json::Value = reply.json().await.expect("the reply is JSON");
    let code = body["code"].as_str().expect("a code");

    let page = reqwest::Client::new()
        .get(format!("{address}/v1/device/approve-page?code={code}"))
        .send()
        .await
        .expect("the request goes");
    assert_eq!(page.status(), 200);
    let text = page.text().await.expect("the page");
    assert!(
        text.contains("dev.conduit.browser.tabs/v1"),
        "the surface is named"
    );
    assert!(text.contains("browser.tabs.read"), "the scope is named");
}

#[tokio::test]
async fn the_extension_presents_its_secret_and_the_broker_accepts_it() {
    let address = broker().await;

    // The extension asks for a code, the user approves it, and the
    // extension gets a secret.
    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/code"))
        .json(&serde_json::json!({
            "surface": "dev.conduit.browser.tabs/v1",
            "scopes": ["browser.tabs.read"],
        }))
        .send()
        .await
        .expect("the request goes");
    let body: serde_json::Value = reply.json().await.expect("the reply is JSON");
    let code = body["code"].as_str().expect("a code").to_string();

    reqwest::Client::new()
        .post(format!("{address}/v1/device/approve"))
        .json(&serde_json::json!({"code": code}))
        .send()
        .await
        .expect("the request goes");

    let reply = reqwest::Client::new()
        .post(format!("{address}/v1/device/secret"))
        .json(&serde_json::json!({"code": code, "instance": "ext-1"}))
        .send()
        .await
        .expect("the request goes");
    let secret = reply.text().await.expect("the secret");

    let socket = address.replace("http://", "ws://");
    let (mut extension, _) = tokio_tungstenite::connect_async(format!(
        "{socket}/v1/tunnel?role=extension&surface=dev.conduit.browser.tabs%2Fv1&secret={secret}&instance=ext-1"
    ))
    .await
    .expect("the extension connects");
    extension
        .send(Message::Text(
            serde_json::to_string(&Frame {
                kind: FrameType::Hello,
                schema: SchemaId::new("dev.conduit.hello", 1),
                channel: "tabs".to_string(),
                call: None,
                seq: 0,
                payload: None,
            })
            .expect("a frame is JSON"),
        ))
        .await
        .expect("the hello goes");

    // The broker answers with a welcome: the secret was good.
    let mut welcome = String::new();
    while !welcome.contains("welcome") {
        let message = extension
            .next()
            .await
            .expect("a message")
            .expect("a message");
        if let Message::Text(text) = message {
            welcome = text.to_string();
        }
    }
    assert!(
        welcome.contains("welcome"),
        "the secret opened the tunnel: {welcome}"
    );
}
