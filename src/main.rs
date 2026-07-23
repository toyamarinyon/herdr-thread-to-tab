use serde_json::json;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use thread_to_tab::codex::CodexClient;
use thread_to_tab::herdr::HerdrClient;

fn required(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("required environment variable {name} is missing"))
}

fn listen() -> Result<(), String> {
    if required("HERDR_ENV")? != "1" {
        return Err("HERDR_ENV must be 1".into());
    }
    let socket_path = required("HERDR_SOCKET_PATH")?;
    let state_dir = required("HERDR_PLUGIN_STATE_DIR")?;
    let herdr = HerdrClient::new(required("HERDR_BIN_PATH")?);
    let codex = CodexClient::new(env::var("CODEX_BIN_PATH").unwrap_or_else(|_| "codex".into()));

    log_sync_errors(thread_to_tab::synchronize_existing(
        Path::new(&state_dir),
        &herdr,
        &codex,
    ));

    let mut socket = UnixStream::connect(&socket_path)
        .map_err(|error| format!("connect Herdr socket: {error}"))?;
    let subscription = json!({
        "id":"thread-to-tab",
        "method":"events.subscribe",
        "params":{"subscriptions":[{"type":"pane.updated"},{"type":"pane.created"}]}
    });
    serde_json::to_writer(&mut socket, &subscription)
        .map_err(|error| format!("encode subscription: {error}"))?;
    socket
        .write_all(b"\n")
        .and_then(|_| socket.flush())
        .map_err(|error| format!("send subscription: {error}"))?;

    // Restored panes can become queryable before their agent metadata and
    // terminal titles are populated. The subscription is already active, so
    // a single delayed pass closes that startup window without losing events.
    std::thread::sleep(Duration::from_secs(1));
    log_sync_errors(thread_to_tab::synchronize_existing(
        Path::new(&state_dir),
        &herdr,
        &codex,
    ));

    for line in BufReader::new(socket).lines() {
        match line {
            Ok(line) => match serde_json::from_str(&line) {
                Ok(value) => {
                    if let Some(pane_id) = thread_to_tab::event_pane_id(&value) {
                        if let Err(error) = thread_to_tab::synchronize_pane(
                            pane_id,
                            Path::new(&state_dir),
                            &herdr,
                            &codex,
                        ) {
                            eprintln!("thread-to-tab: event skipped: {error}");
                        }
                    }
                }
                Err(error) => eprintln!("thread-to-tab: malformed event skipped: {error}"),
            },
            Err(error) => eprintln!("thread-to-tab: unreadable event skipped: {error}"),
        }
    }
    Err("Herdr event socket closed".into())
}

fn log_sync_errors(errors: Vec<String>) {
    for error in errors {
        eprintln!("thread-to-tab: startup sync skipped: {error}");
    }
}

fn main() {
    let result = if env::args().skip(1).eq(["--listen"]) {
        listen()
    } else {
        Err("usage: thread-to-tab --listen".into())
    };
    if let Err(error) = result {
        eprintln!("thread-to-tab: {error}");
        std::process::exit(1);
    }
}
