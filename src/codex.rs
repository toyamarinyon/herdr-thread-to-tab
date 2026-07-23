use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Thread {
    pub name: Option<String>,
    pub preview: Option<String>,
}

pub trait CodexApi {
    fn thread(&self, thread_id: &str) -> Result<Thread, String>;
}

pub struct CodexClient {
    executable: String,
    timeout: Duration,
}

impl CodexClient {
    pub fn new(executable: String) -> Self {
        Self {
            executable,
            timeout: REQUEST_TIMEOUT,
        }
    }

    #[cfg(test)]
    fn with_timeout(executable: String, timeout: Duration) -> Self {
        Self {
            executable,
            timeout,
        }
    }
}

impl CodexApi for CodexClient {
    fn thread(&self, thread_id: &str) -> Result<Thread, String> {
        let mut child = Command::new(&self.executable)
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("could not start Codex app-server: {error}"))?;
        let result = exchange(&mut child, thread_id, self.timeout);
        terminate_and_reap(&mut child);
        result
    }
}

fn exchange(child: &mut Child, thread_id: &str, timeout: Duration) -> Result<Thread, String> {
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| "Codex stdin unavailable".to_owned())?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| "Codex stdout unavailable".to_owned())?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    send(
        &mut input,
        &json!({"method":"initialize","id":0,"params":{"clientInfo":{
            "name":"toyamarinyon_thread_to_tab","title":"Thread to Tab","version":"0.1.0"
        }}}),
    )?;
    response(&receiver, 0, timeout)?;
    send(&mut input, &json!({"method":"initialized","params":{}}))?;
    send(
        &mut input,
        &json!({"method":"thread/read","id":1,"params":{
            "threadId":thread_id,"includeTurns":false
        }}),
    )?;
    let value = response(&receiver, 1, timeout)?;
    let thread = value
        .get("result")
        .and_then(|result| result.get("thread"))
        .cloned()
        .ok_or_else(|| "Codex thread/read response has no thread".to_owned())?;
    serde_json::from_value(thread).map_err(|error| format!("invalid Codex thread: {error}"))
}

fn send(input: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *input, value)
        .map_err(|error| format!("encode Codex request: {error}"))?;
    input
        .write_all(b"\n")
        .and_then(|_| input.flush())
        .map_err(|error| format!("write Codex request: {error}"))
}

fn response(
    receiver: &Receiver<std::io::Result<String>>,
    id: u64,
    timeout: Duration,
) -> Result<Value, String> {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let line = receiver
            .recv_timeout(remaining)
            .map_err(|_| format!("Codex request {id} timed out"))?
            .map_err(|error| format!("read Codex response: {error}"))?;
        if let Ok(value) = serde_json::from_str::<Value>(&line) {
            if value.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = value.get("error") {
                    return Err(format!("Codex request {id} failed: {error}"));
                }
                return Ok(value);
            }
        }
    }
}

fn terminate_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn exchanges_initialize_and_thread_read_jsonl() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-codex");
        fs::write(
            &script,
            r#"#!/bin/sh
read initialize
printf '%s\n' '{"id":0,"result":{}}'
read initialized
read thread_read
printf '%s\n' '{"id":1,"result":{"thread":{"name":"Fake name","preview":"Fake preview"}}}'
"#,
        )
        .unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let client = CodexClient::new(script.to_string_lossy().into_owned());
        let thread = client.thread("thread-1").unwrap();
        assert_eq!(thread.name.as_deref(), Some("Fake name"));
        assert_eq!(thread.preview.as_deref(), Some("Fake preview"));
    }

    #[test]
    fn times_out_a_hung_app_server() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("hung-codex");
        fs::write(&script, "#!/bin/sh\nsleep 10\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();

        let client = CodexClient::with_timeout(
            script.to_string_lossy().into_owned(),
            Duration::from_millis(50),
        );
        let started = Instant::now();
        let error = client.thread("thread-1").unwrap_err();
        assert!(error.contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
