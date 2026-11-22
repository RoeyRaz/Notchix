//! Optional, explicitly triggered Notion task creation. The token is protected
//! with Windows DPAPI by SETUP-NOTION.ps1 and never enters a command line.
pub fn configured() -> bool {
    let dir = crate::store::data_dir();
    dir.join("notion-config.json").exists() && dir.join("notion-token.dpapi").exists()
}

pub fn create_task(title: &str) -> String {
    if !configured() { return "Notion is not connected. Run SETUP-NOTION.ps1 first.".into(); }
    let title = title.trim();
    if title.is_empty() { return "Write a task before sending it to Notion.".into(); }
    let path = crate::store::data_dir().join(format!("notion-task-{}.json", std::process::id()));
    let body = serde_json::json!({ "title": title.chars().take(2000).collect::<String>() }).to_string();
    if std::fs::write(&path, body).is_err() { return "Could not prepare the Notion task.".into(); }
    let script = include_str!("../assets/notion_task.ps1").replace("{PID}", &std::process::id().to_string());
    let answer = crate::sys::powershell(&script).unwrap_or_else(|| "Notion task was not created.".into());
    let _ = std::fs::remove_file(path);
    answer.trim().to_string()
}
