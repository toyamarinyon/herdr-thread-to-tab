use serde::Deserialize;
use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use wait_timeout::ChildExt;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentSession {
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Pane {
    pub agent: Option<String>,
    pub agent_session: Option<AgentSession>,
    pub cwd: Option<String>,
    pub foreground_cwd: Option<String>,
    pub pane_id: Option<String>,
    pub tab_id: Option<String>,
    pub terminal_id: Option<String>,
    pub terminal_title_stripped: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Tab {
    pub label: Option<String>,
    pub pane_count: Option<u64>,
}

pub trait HerdrApi {
    fn pane(&self, pane_id: &str) -> Result<Pane, String>;
    fn panes(&self) -> Result<Vec<Pane>, String>;
    fn tab(&self, tab_id: &str) -> Result<Tab, String>;
    fn rename_tab(&self, tab_id: &str, label: &str) -> Result<(), String>;
}

pub struct HerdrClient {
    executable: String,
}

impl HerdrClient {
    pub fn new(executable: String) -> Self {
        Self { executable }
    }

    fn json(&self, operation: &str, arguments: &[&str]) -> Result<Value, String> {
        let output = run_bounded(&self.executable, arguments, COMMAND_TIMEOUT)
            .map_err(|error| format!("{operation}: {error}"))?;
        serde_json::from_slice(&output)
            .map_err(|error| format!("{operation}: invalid JSON: {error}"))
    }
}

impl HerdrApi for HerdrClient {
    fn pane(&self, pane_id: &str) -> Result<Pane, String> {
        let value = self.json("pane get", &["pane", "get", pane_id])?;
        parse_nested(value, "pane", "pane get")
    }

    fn panes(&self) -> Result<Vec<Pane>, String> {
        let value = self.json("pane list", &["pane", "list"])?;
        let value = value.get("result").unwrap_or(&value);
        serde_json::from_value(value.get("panes").cloned().unwrap_or(Value::Null))
            .map_err(|error| format!("pane list: invalid panes: {error}"))
    }

    fn tab(&self, tab_id: &str) -> Result<Tab, String> {
        let value = self.json("tab get", &["tab", "get", tab_id])?;
        parse_nested(value, "tab", "tab get")
    }

    fn rename_tab(&self, tab_id: &str, label: &str) -> Result<(), String> {
        run_bounded(
            &self.executable,
            &["tab", "rename", tab_id, label],
            COMMAND_TIMEOUT,
        )
        .map(|_| ())
        .map_err(|error| format!("tab rename: {error}"))
    }
}

fn parse_nested<T: for<'de> Deserialize<'de>>(
    value: Value,
    key: &str,
    operation: &str,
) -> Result<T, String> {
    let value = value.get("result").unwrap_or(&value);
    let value = value.get(key).unwrap_or(value);
    serde_json::from_value(value.clone())
        .map_err(|error| format!("{operation}: invalid {key}: {error}"))
}

pub fn run_bounded(
    executable: &str,
    arguments: &[&str],
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start: {error}"))?;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let mut stderr = child.stderr.take().expect("piped stderr");
    let out_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let err_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let status = match child
        .wait_timeout(timeout)
        .map_err(|error| format!("wait failed: {error}"))?
    {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_reader.join();
            let _ = err_reader.join();
            return Err(format!("timed out after {}s", timeout.as_secs()));
        }
    };
    let stdout = out_reader
        .join()
        .map_err(|_| "stdout reader failed".to_owned())?
        .map_err(|error| format!("read stdout: {error}"))?;
    let stderr = err_reader
        .join()
        .map_err(|_| "stderr reader failed".to_owned())?
        .map_err(|error| format!("read stderr: {error}"))?;
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr);
        let concise = message.trim().chars().take(500).collect::<String>();
        return Err(if concise.is_empty() {
            format!("exited with {status}")
        } else {
            format!("exited with {status}: {concise}")
        });
    }
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_wrapped_cli_json() {
        let pane: Pane = parse_nested(
            serde_json::json!({"result": {"pane": {"pane_id": "1-1", "agent": "claude"}}}),
            "pane",
            "pane get",
        )
        .unwrap();
        assert_eq!(pane.pane_id.as_deref(), Some("1-1"));
        assert_eq!(pane.agent.as_deref(), Some("claude"));
    }

    #[cfg(unix)]
    #[test]
    fn bounded_command_is_killed_on_timeout() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("slow-command");
        fs::write(&script, "#!/bin/sh\nsleep 10\n").unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script, permissions).unwrap();
        let error =
            run_bounded(script.to_str().unwrap(), &[], Duration::from_millis(50)).unwrap_err();
        assert!(error.contains("timed out"));
    }
}
