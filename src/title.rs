use crate::codex::CodexApi;
use crate::herdr::Pane;

const GENERIC: &[&str] = &["claude", "claude code", "codex"];

pub fn clean(value: Option<&str>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    if value.chars().any(char::is_control) {
        return String::new();
    }
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_owned();
    }
    let keep = max.saturating_sub(3);
    let mut result = value.chars().take(keep).collect::<String>();
    while result.ends_with(char::is_whitespace) {
        result.pop();
    }
    result.push_str("...");
    result
}

fn is_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        if [8, 13, 18, 23].contains(&index) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn project_name(pane: &Pane) -> Option<&str> {
    pane.foreground_cwd
        .as_deref()
        .or(pane.cwd.as_deref())
        .and_then(|path| path.rsplit('/').find(|part| !part.is_empty()))
}

fn rejected_codex_fallback(title: &str, pane: &Pane) -> bool {
    if is_uuid(&title.to_ascii_lowercase()) {
        return true;
    }
    let Some(project) = project_name(pane) else {
        return false;
    };
    let title = title.to_lowercase();
    let project = project.to_lowercase();
    title == project
        || title
            .strip_suffix("...")
            .is_some_and(|prefix| project.starts_with(prefix))
}

pub fn candidate(pane: &Pane, codex: &dyn CodexApi, max: usize) -> String {
    let Some(agent) = pane.agent.as_deref() else {
        return String::new();
    };
    if agent != "claude" && agent != "codex" {
        return String::new();
    }
    if agent == "codex" {
        if let Some(thread_id) = pane
            .agent_session
            .as_ref()
            .and_then(|session| session.value.as_deref())
        {
            if let Ok(thread) = codex.thread(thread_id) {
                for value in [thread.name.as_deref(), thread.preview.as_deref()] {
                    let value = clean(value);
                    if !value.is_empty() {
                        return truncate(&value, max);
                    }
                }
            }
        }
    }
    let fallback = clean(pane.terminal_title_stripped.as_deref());
    if fallback.is_empty()
        || GENERIC
            .iter()
            .any(|generic| fallback.eq_ignore_ascii_case(generic))
        || (agent == "codex" && rejected_codex_fallback(&fallback, pane))
    {
        return String::new();
    }
    truncate(&fallback, max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::{CodexApi, Thread};
    use crate::herdr::{AgentSession, Pane};

    struct Fake(Result<Thread, String>);
    impl CodexApi for Fake {
        fn thread(&self, _: &str) -> Result<Thread, String> {
            self.0.clone()
        }
    }

    fn codex_pane(title: &str) -> Pane {
        Pane {
            agent: Some("codex".into()),
            agent_session: Some(AgentSession {
                value: Some("thread-1".into()),
            }),
            cwd: Some("/workspace/project".into()),
            terminal_title_stripped: Some(title.into()),
            ..Pane::default()
        }
    }

    #[test]
    fn cleans_whitespace_but_rejects_controls() {
        assert_eq!(clean(Some("  hello   world  ")), "hello world");
        assert_eq!(clean(Some("hello\nworld")), "");
        assert_eq!(clean(Some("hello\u{7f}world")), "");
    }

    #[test]
    fn truncates_unicode_scalars_to_thirty() {
        let value = "日".repeat(40);
        let result = truncate(&value, 30);
        assert_eq!(result.chars().count(), 30);
        assert_eq!(result, format!("{}...", "日".repeat(27)));
    }

    #[test]
    fn codex_order_and_fallback() {
        let pane = codex_pane("terminal fallback");
        let named = Fake(Ok(Thread {
            name: Some("Named thread".into()),
            preview: Some("Preview".into()),
        }));
        assert_eq!(candidate(&pane, &named, 30), "Named thread");
        let preview = Fake(Ok(Thread {
            name: None,
            preview: Some("Preview".into()),
        }));
        assert_eq!(candidate(&pane, &preview, 30), "Preview");
        let failed = Fake(Err("missing".into()));
        assert_eq!(candidate(&pane, &failed, 30), "terminal fallback");
    }

    #[test]
    fn rejects_generic_uuid_project_and_unsupported() {
        let none = Fake(Err("missing".into()));
        assert_eq!(candidate(&codex_pane("Codex"), &none, 30), "");
        assert_eq!(
            candidate(
                &codex_pane("019f8e03-5bbd-70a1-9017-b2216a66bf4c"),
                &none,
                30
            ),
            ""
        );
        assert_eq!(candidate(&codex_pane("project"), &none, 30), "");
        let mut pane = codex_pane("project...");
        pane.cwd = Some("/workspace/project-long".into());
        assert_eq!(candidate(&pane, &none, 30), "");
        pane.agent = Some("shell".into());
        assert_eq!(candidate(&pane, &none, 30), "");
    }
}
