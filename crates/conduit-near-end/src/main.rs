//! The near end, run as a program.
//!
//! It is told what to carry by a configuration file:
//!
//! ```text
//! conduit-near-end ~/.config/conduit/near-end.json
//! ```
//!
//! The path may also come from the environment `CONDUIT_NEAR_END_CONFIG`,
//! which is what a desktop entry or a launch agent would set.

use conduit_near_end::{run, run_native, Config};

/// Where the configuration is.
///
/// The path may be given as an argument, or in the environment.  The
/// flags are skipped: `--native` is one of them.
fn the_configuration_path() -> Option<String> {
    if let Ok(path) = std::env::var("CONDUIT_NEAR_END_CONFIG") {
        if !path.is_empty() {
            return Some(path);
        }
    }
    std::env::args()
        .skip(1)
        .find(|argument| !argument.starts_with("--"))
}

#[tokio::main]
async fn main() {
    // The native mode: a browser's native messaging host.  The near end
    // is started by the browser, which is told where the broker is and
    // what the secret is.
    let native = std::env::args().any(|argument| argument == "--native");

    let Some(path) = the_configuration_path() else {
        eprintln!("conduit-near-end: no configuration; give a path or set CONDUIT_NEAR_END_CONFIG");
        std::process::exit(2);
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("conduit-near-end: {path} could not be read ({error})");
            std::process::exit(2);
        }
    };

    let config: Config = match serde_json::from_str(&text) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("conduit-near-end: {path} is not a configuration ({error})");
            std::process::exit(2);
        }
    };

    if native {
        if let Err(error) = run_native(config).await {
            eprintln!("conduit-near-end: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run(config).await {
        eprintln!("conduit-near-end: {error}");
        std::process::exit(1);
    }
}
