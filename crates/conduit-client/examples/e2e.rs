//! A client for the end-to-end run: connects to a broker, subscribes,
//! and prints everything it gets as JSON lines on stdout.
//!
//! ```sh
//! cargo run -p conduit-client --example e2e -- \
//!   --broker ws://127.0.0.1:8787/v1/tunnel --token bootstrap-token \
//!   --address conduit://dev.conduit.browser.tabs/tabs@v1 --scope browser.tabs.read
//! ```
//!
//! `--flood N` => after the first event, publish N frames as fast as the
//! socket takes them and count what the broker answers; that's the 200
//! frames/s limit seen from a real client.
//!
//! lines: `connected`, `subscribe`, `event`, `refused`, `flood`,
//! `after_refill`, `closed`, `fatal`.

use conduit_client::{Address, Client, ClientError, Event};
use conduit_protocol::{Frame, SchemaId, Scope};
use serde_json::{json, Value};
use std::time::Duration;

struct Options {
    broker: String,
    token: String,
    address: String,
    scopes: Vec<String>,
    flood: Option<usize>,
}

fn options() -> Result<Options, String> {
    let mut options = Options {
        broker: String::new(),
        token: String::new(),
        address: String::new(),
        scopes: Vec::new(),
        flood: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--broker" => options.broker = value()?,
            "--token" => options.token = value()?,
            "--address" => options.address = value()?,
            "--scope" => options.scopes.push(value()?),
            "--flood" => {
                options.flood = Some(value()?.parse().map_err(|_| "--flood takes a number")?)
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    if options.broker.is_empty() || options.token.is_empty() || options.address.is_empty() {
        return Err("--broker, --token and --address are required".to_string());
    }
    Ok(options)
}

fn line(value: Value) {
    println!("{value}");
}

fn event_line(event: &Event) {
    let payload = serde_json::from_str::<Value>(&event.payload)
        .unwrap_or_else(|_| Value::String(event.payload.clone()));
    line(json!({
        "type": "event",
        "schema": event.schema.to_string(),
        "channel": event.channel,
        "seq": event.seq,
        "payload": payload,
        "sealed": event.sealed,
    }));
}

/// The error inside a refusal frame, as the broker wrote it.
fn refusal(text: &str) -> Value {
    let error = serde_json::from_str::<Frame>(text)
        .ok()
        .and_then(|frame| frame.payload)
        .and_then(|payload| serde_json::from_str::<Value>(&payload).ok())
        .unwrap_or(Value::Null);
    json!({
        "type": "refused",
        "code": error.get("code").cloned().unwrap_or(Value::Null),
        "scope": error.get("scope").cloned().unwrap_or(Value::Null),
        "message": error.get("message").cloned().unwrap_or(Value::Null),
    })
}

/// What one read of the tunnel gave.
enum Next {
    Event(Event),
    Refused(Value),
    Closed,
    Quiet,
}

async fn next(client: &mut Client, wait: Duration) -> Result<Next, ClientError> {
    match tokio::time::timeout(wait, client.next()).await {
        Err(_) => Ok(Next::Quiet),
        Ok(Ok(Some(event))) => Ok(Next::Event(event)),
        Ok(Ok(None)) => Ok(Next::Closed),
        Ok(Err(ClientError::Refused(text))) => Ok(Next::Refused(refusal(&text))),
        Ok(Err(ClientError::Connection(_))) => Ok(Next::Closed),
        Ok(Err(error)) => Err(error),
    }
}

/// Publish `count` frames, then count the refusals that come back.
async fn flood(client: &mut Client, address: &Address, count: usize) -> Result<bool, ClientError> {
    let schema = SchemaId::new("dev.conduit.browser.tab", 1);
    let channel = address.channel();
    let began = std::time::Instant::now();
    for index in 0..count {
        let payload = json!({ "flood": index }).to_string();
        client
            .publish(&schema, &channel, payload.as_bytes())
            .await?;
    }
    let elapsed_ms = began.elapsed().as_millis();
    let mut refused = 0usize;
    let mut codes = std::collections::BTreeMap::<String, usize>::new();
    loop {
        match next(client, Duration::from_millis(1500)).await? {
            Next::Refused(value) => {
                refused += 1;
                let code = value["code"].as_str().unwrap_or("?").to_string();
                *codes.entry(code).or_default() += 1;
            }
            Next::Event(event) => event_line(&event),
            Next::Quiet => break,
            Next::Closed => {
                line(json!({ "type": "closed" }));
                return Ok(false);
            }
        }
    }
    line(
        json!({ "type": "flood", "sent": count, "elapsed_ms": elapsed_ms, "refused": refused, "codes": codes }),
    );

    // a second later the bucket is full again, and the socket is open.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    client
        .publish(
            &schema,
            &channel,
            json!({ "flood": "after" }).to_string().as_bytes(),
        )
        .await?;
    let mut answer = Value::Null;
    loop {
        match next(client, Duration::from_secs(3)).await? {
            Next::Refused(value) => {
                answer = value;
                break;
            }
            Next::Event(event) => event_line(&event),
            Next::Quiet => break,
            Next::Closed => {
                line(json!({ "type": "closed" }));
                return Ok(false);
            }
        }
    }
    line(json!({ "type": "after_refill", "answer": answer }));
    Ok(true)
}

async fn run(options: Options) -> Result<(), Box<dyn std::error::Error>> {
    let address = Address::parse(&options.address)?;
    let mut client = Client::connect(&options.broker, &options.token, &address).await?;
    line(json!({ "type": "connected", "surface": address.surface().to_string() }));

    let scopes: Vec<Scope> = options
        .scopes
        .iter()
        .filter_map(|scope| scope.rsplit_once('.'))
        .map(|(namespace, verb)| Scope::new(namespace, verb))
        .collect();
    match client.subscribe(&address, &scopes).await {
        Ok(subscription) => line(json!({
            "type": "subscribe",
            "channel": subscription.channel,
            "scopes": options.scopes,
        })),
        Err(ClientError::Refused(text)) => line(refusal(&text)),
        Err(error) => return Err(error.into()),
    }

    let mut flood_left = options.flood;
    loop {
        match next(&mut client, Duration::from_secs(3600)).await? {
            Next::Event(event) => {
                event_line(&event);
                if let Some(count) = flood_left.take() {
                    if !flood(&mut client, &address, count).await? {
                        return Ok(());
                    }
                }
            }
            Next::Refused(value) => line(value),
            Next::Quiet => {}
            Next::Closed => {
                line(json!({ "type": "closed" }));
                return Ok(());
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let options = match options() {
        Ok(options) => options,
        Err(error) => {
            line(json!({ "type": "fatal", "error": error }));
            std::process::exit(2);
        }
    };
    if let Err(error) = run(options).await {
        line(json!({ "type": "fatal", "error": error.to_string() }));
        std::process::exit(1);
    }
}
