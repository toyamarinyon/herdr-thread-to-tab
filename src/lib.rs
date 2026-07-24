pub mod codex;
pub mod herdr;
pub mod state;
pub mod title;

use std::collections::HashMap;
use std::path::Path;

use codex::CodexApi;
use herdr::{HerdrApi, Pane};
use state::LockedState;

pub const MAX_TITLE_LENGTH: usize = 30;

pub fn candidate_title(pane: &Pane, codex: &dyn CodexApi) -> String {
    title::candidate(pane, codex, MAX_TITLE_LENGTH)
}

pub fn synchronize_pane(
    pane_id: &str,
    state_dir: &Path,
    herdr: &dyn HerdrApi,
    codex: &dyn CodexApi,
) -> Result<bool, String> {
    if pane_id.is_empty() {
        return Ok(false);
    }
    let pane = herdr.pane(pane_id)?;
    let next = candidate_title(&pane, codex);
    if next.is_empty() {
        return Ok(false);
    }
    let (tab_id, terminal_id) = match (pane.tab_id.as_deref(), pane.terminal_id.as_deref()) {
        (Some(tab), Some(terminal)) => (tab, terminal),
        _ => return Ok(false),
    };
    let tab = herdr.tab(tab_id)?;
    if tab.pane_count != Some(1) {
        return Ok(false);
    }
    let current = match tab.label {
        Some(value) => value,
        None => return Ok(false),
    };

    let state_path = state_dir.join("titles.json");
    let mut locked = LockedState::open(&state_path)?;
    let owned = (!current.is_empty() && current.chars().all(|c| c.is_ascii_digit()))
        || locked.values().any(|value| value == &current);
    if current == next {
        if owned {
            locked.insert(terminal_id.to_owned(), next);
            locked.save()?;
        }
        return Ok(false);
    }
    if !owned {
        return Ok(false);
    }
    herdr.rename_tab(tab_id, &next)?;
    locked.insert(terminal_id.to_owned(), next);
    locked.save()?;
    Ok(true)
}

pub fn event_pane_id(value: &serde_json::Value) -> Option<&str> {
    // Streamed events arrive as {"event":"pane_updated","data":{"type":"pane_updated","pane":{...}}}
    // while subscription requests use dot names; accept both spellings.
    if matches!(
        value.get("type").and_then(|v| v.as_str()),
        Some("pane.created" | "pane.updated" | "pane_created" | "pane_updated")
    ) {
        return value
            .get("pane")
            .and_then(|pane| pane.get("pane_id"))
            .and_then(|id| id.as_str());
    }
    for key in ["data", "event", "result"] {
        if let Some(id) = value.get(key).and_then(event_pane_id) {
            return Some(id);
        }
    }
    None
}

pub fn synchronize_existing(
    state_dir: &Path,
    herdr: &dyn HerdrApi,
    codex: &dyn CodexApi,
) -> Vec<String> {
    match herdr.panes() {
        Ok(panes) => panes
            .into_iter()
            .filter_map(|pane| {
                let id = pane.pane_id?;
                synchronize_pane(&id, state_dir, herdr, codex)
                    .err()
                    .map(|error| format!("pane {id}: {error}"))
            })
            .collect(),
        Err(error) => vec![format!("pane list: {error}")],
    }
}

pub fn state_from_json(bytes: &[u8]) -> HashMap<String, String> {
    serde_json::from_slice(bytes).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::{CodexApi, Thread};
    use crate::herdr::{Pane, Tab};
    use std::cell::RefCell;

    struct FakeCodex(Thread);
    impl CodexApi for FakeCodex {
        fn thread(&self, _: &str) -> Result<Thread, String> {
            Ok(self.0.clone())
        }
    }

    struct FakeHerdr {
        pane: Pane,
        tab: RefCell<Tab>,
        renames: RefCell<Vec<(String, String)>>,
    }
    impl HerdrApi for FakeHerdr {
        fn pane(&self, _: &str) -> Result<Pane, String> {
            Ok(self.pane.clone())
        }
        fn panes(&self) -> Result<Vec<Pane>, String> {
            Ok(vec![self.pane.clone()])
        }
        fn tab(&self, _: &str) -> Result<Tab, String> {
            Ok(self.tab.borrow().clone())
        }
        fn rename_tab(&self, id: &str, label: &str) -> Result<(), String> {
            self.renames
                .borrow_mut()
                .push((id.to_owned(), label.to_owned()));
            self.tab.borrow_mut().label = Some(label.to_owned());
            Ok(())
        }
    }

    fn pane() -> Pane {
        Pane {
            agent: Some("claude".into()),
            pane_id: Some("1-1".into()),
            tab_id: Some("1:1".into()),
            terminal_id: Some("terminal-1".into()),
            terminal_title_stripped: Some("Refactor auth middleware".into()),
            ..Pane::default()
        }
    }

    fn herdr(pane: Pane, label: &str, count: u64) -> FakeHerdr {
        FakeHerdr {
            pane,
            tab: RefCell::new(Tab {
                label: Some(label.into()),
                pane_count: Some(count),
            }),
            renames: RefCell::new(Vec::new()),
        }
    }

    #[test]
    fn synchronization_ownership_rules() {
        let dir = tempfile::tempdir().unwrap();
        let api = herdr(pane(), "1", 1);
        let codex = FakeCodex(Thread::default());
        assert!(synchronize_pane("1-1", dir.path(), &api, &codex).unwrap());
        api.pane
            .terminal_title_stripped
            .as_ref()
            .expect("title exists");
        let mut changed_pane = api.pane.clone();
        changed_pane.terminal_id = Some("terminal-2".into());
        changed_pane.terminal_title_stripped = Some("Implement OAuth".into());
        let api2 = herdr(changed_pane, "Refactor auth middleware", 1);
        assert!(synchronize_pane("1-1", dir.path(), &api2, &codex).unwrap());

        let manual = herdr(pane(), "manual", 1);
        assert!(!synchronize_pane("1-1", dir.path(), &manual, &codex).unwrap());
        let manual_dir = tempfile::tempdir().unwrap();
        let matching_manual = herdr(pane(), "Refactor auth middleware", 1);
        assert!(!synchronize_pane("1-1", manual_dir.path(), &matching_manual, &codex).unwrap());
        let state = std::fs::read(manual_dir.path().join("titles.json")).unwrap();
        assert!(state_from_json(&state).is_empty());
        let multi = herdr(pane(), "1", 2);
        assert!(!synchronize_pane("1-1", dir.path(), &multi, &codex).unwrap());
    }

    #[test]
    fn accepts_python_state_shape() {
        let state = state_from_json(br#"{"old-terminal":"Previous title"}"#);
        assert_eq!(state.get("old-terminal").unwrap(), "Previous title");
    }

    #[test]
    fn extracts_nested_events() {
        let event = serde_json::json!({
            "result": {"event": {"type": "pane.updated", "pane": {"pane_id": "1-2"}}}
        });
        assert_eq!(event_pane_id(&event), Some("1-2"));
        assert_eq!(
            event_pane_id(&serde_json::json!({"type": "tab.renamed"})),
            None
        );
    }

    #[test]
    fn extracts_streamed_subscription_events() {
        let updated = serde_json::json!({
            "event": "pane_updated",
            "data": {"type": "pane_updated", "pane": {"pane_id": "w23:p1"}}
        });
        assert_eq!(event_pane_id(&updated), Some("w23:p1"));
        let created = serde_json::json!({
            "event": "pane_created",
            "data": {"type": "pane_created", "pane": {"pane_id": "w24:p1"}}
        });
        assert_eq!(event_pane_id(&created), Some("w24:p1"));
        let ack = serde_json::json!({
            "id": "thread-to-tab",
            "result": {"type": "subscription_started"}
        });
        assert_eq!(event_pane_id(&ack), None);
    }
}
