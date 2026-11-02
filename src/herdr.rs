//! Calls back into herdr over its CLI: reading a pane's native agent session
//! (with a short poll for the documented timing race) and renaming Herdr
//! labels on the pane, its tab, and the workspace.

use std::env;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

const METADATA_SOURCE: &str = "plugin:herdr-plugin-renamer";

fn herdr_bin() -> String {
    env::var("HERDR_BIN_PATH").unwrap_or_else(|_| "herdr".to_string())
}

/// `herdr pane get <pane_id>` returns JSON by default. Returns the pane object,
/// accepting the CLI's `{"result":{"pane":{...}}}` envelope first and falling
/// back to unwrapped variants. `None` when the CLI fails or the JSON is
/// unreadable.
fn pane_get(pane_id: &str) -> Option<serde_json::Value> {
    let output = Command::new(herdr_bin())
        .args(["pane", "get", pane_id])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    value
        .pointer("/result/pane")
        .or_else(|| value.get("pane"))
        .cloned()
        .unwrap_or(value)
        .into()
}

/// Extract the agent label and session reference (an id or Pi-reported path)
/// from the pane's `agent_session`. Returns `None` when the session has not
/// been reported yet.
fn pane_agent_session(pane_id: &str) -> Option<(String, String)> {
    let pane = pane_get(pane_id)?;
    let session = pane.get("agent_session")?;

    let agent = match session.get("agent").and_then(|a| a.as_str()) {
        Some(a) => a.to_string(),
        // Older builds emit only `source` (e.g. "herdr:claude").
        None => session
            .get("source")
            .and_then(|s| s.as_str())
            .map(|s| s.trim_start_matches("herdr:").to_string())?,
    };
    let value = session.get("value").and_then(|v| v.as_str())?.to_string();
    Some((agent, value))
}

/// Poll `pane get` for the session reference. `pane.agent_status_changed` can fire
/// before herdr has received the session from the integration hook, so we retry
/// briefly before giving up.
pub fn poll_agent_session(
    pane_id: &str,
    attempts: u32,
    delay: Duration,
) -> Option<(String, String)> {
    for attempt in 0..attempts {
        if let Some(session) = pane_agent_session(pane_id) {
            return Some(session);
        }
        if attempt + 1 < attempts {
            sleep(delay);
        }
    }
    None
}

/// `herdr workspace rename <workspace_id> <label>`.
pub fn workspace_rename(workspace_id: &str, label: &str) -> bool {
    Command::new(herdr_bin())
        .args(["workspace", "rename", workspace_id, label])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// `herdr pane rename <pane_id> <label>`.
pub fn pane_rename(pane_id: &str, label: &str) -> bool {
    Command::new(herdr_bin())
        .args(["pane", "rename", pane_id, label])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// The id of the tab holding this pane. Herdr reports the parent tab directly
/// on the pane object, so no listing or derivation is needed.
fn tab_id_from_pane_json(pane: &serde_json::Value) -> Option<String> {
    pane.get("tab_id")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

/// The tab holding `pane_id`, for tab renames.
pub fn pane_tab_id(pane_id: &str) -> Option<String> {
    let pane = pane_get(pane_id)?;
    tab_id_from_pane_json(&pane)
}

/// `herdr tab rename <tab_id> <label>`.
pub fn tab_rename(tab_id: &str, label: &str) -> bool {
    Command::new(herdr_bin())
        .args(["tab", "rename", tab_id, label])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Publish the generated task name for custom Agent sidebar rows via `$task`,
/// and also set pane title / display-agent so the Herdr UI and outer title
/// plugins can show the task without scraping session files.
pub fn pane_report_task(pane_id: &str, task: &str) -> bool {
    report_task_metadata("pane", pane_id, task, true)
}

/// Publish the generated task name for custom Space sidebar rows via `$task`.
pub fn workspace_report_task(workspace_id: &str, task: &str) -> bool {
    report_task_metadata("workspace", workspace_id, task, false)
}

fn report_task_metadata(resource: &str, resource_id: &str, task: &str, with_title: bool) -> bool {
    let token = format!("task={task}");
    let mut args = vec![
        resource.to_string(),
        "report-metadata".to_string(),
        resource_id.to_string(),
        "--source".to_string(),
        METADATA_SOURCE.to_string(),
        "--token".to_string(),
        token,
    ];
    // Pane metadata can carry a human title + display-agent. Workspace
    // metadata only supports tokens for custom Space rows.
    if with_title {
        args.push("--title".to_string());
        args.push(task.to_string());
        args.push("--display-agent".to_string());
        args.push(task.to_string());
        // Keep the title sticky for the session; cold phase refreshes it.
        args.push("--ttl-ms".to_string());
        args.push("86400000".to_string());
    }
    Command::new(herdr_bin())
        .args(&args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::tab_id_from_pane_json;

    #[test]
    fn tab_id_is_read_from_the_pane_object() {
        let pane = serde_json::json!({"pane_id": "w5:pN", "tab_id": "w5:tK"});
        assert_eq!(tab_id_from_pane_json(&pane).as_deref(), Some("w5:tK"));
    }

    #[test]
    fn missing_blank_or_non_string_tab_ids_are_none() {
        let cases = [
            serde_json::json!({"pane_id": "w5:pN"}),
            serde_json::json!({"tab_id": ""}),
            serde_json::json!({"tab_id": 7}),
        ];
        for pane in cases {
            assert_eq!(tab_id_from_pane_json(&pane), None);
        }
    }
}
