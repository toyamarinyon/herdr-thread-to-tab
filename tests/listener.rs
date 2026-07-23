#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::Command;
use std::thread;

#[test]
fn listener_subscribes_skips_malformed_event_and_syncs() {
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("herdr.sock");
    let listener = UnixListener::bind(&socket_path).unwrap();
    let rename_log = directory.path().join("rename.log");
    let fake_herdr = directory.path().join("fake-herdr");
    fs::write(
        &fake_herdr,
        format!(
            r#"#!/bin/sh
case "$1 $2" in
  "pane list") printf '%s\n' '{{"result":{{"panes":[]}}}}' ;;
  "pane get") printf '%s\n' '{{"result":{{"pane":{{"agent":"claude","pane_id":"1-1","tab_id":"1:1","terminal_id":"term-1","terminal_title_stripped":"Listener title"}}}}}}' ;;
  "tab get") printf '%s\n' '{{"result":{{"tab":{{"label":"1","pane_count":1}}}}}}' ;;
  "tab rename") printf '%s|%s\n' "$3" "$4" > '{}' ;;
  *) exit 2 ;;
esac
"#,
            rename_log.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_herdr).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_herdr, permissions).unwrap();

    let server = thread::spawn(move || {
        let (mut connection, _) = listener.accept().unwrap();
        let mut request = String::new();
        BufReader::new(connection.try_clone().unwrap())
            .read_line(&mut request)
            .unwrap();
        let request: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["method"], "events.subscribe");
        connection.write_all(b"not-json\n").unwrap();
        connection
            .write_all(b"{\"event\":{\"type\":\"pane.updated\",\"pane\":{\"pane_id\":\"1-1\"}}}\n")
            .unwrap();
    });

    let binary = std::env::var_os("THREAD_TO_TAB_TEST_LISTENER_BINARY")
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_thread-to-tab").into());
    let output = Command::new(binary)
        .arg("--listen")
        .env("HERDR_ENV", "1")
        .env("HERDR_SOCKET_PATH", &socket_path)
        .env("HERDR_PLUGIN_STATE_DIR", directory.path().join("state"))
        .env("HERDR_BIN_PATH", &fake_herdr)
        .output()
        .unwrap();
    server.join().unwrap();

    assert!(!output.status.success(), "socket closure must be non-zero");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("malformed event skipped"));
    assert!(stderr.contains("event socket closed"));
    assert_eq!(
        fs::read_to_string(rename_log).unwrap(),
        "1:1|Listener title\n"
    );
    let state = fs::read_to_string(directory.path().join("state/titles.json")).unwrap();
    assert!(state.contains("\"term-1\": \"Listener title\""));
}
