#![allow(non_snake_case)]

use crate::session_manager;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
const SUPPORTED_PROVIDERS: [&str; 6] = [
    "codex", "claude", "opencode", "openclaw", "gemini", "hermes",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncRequest {
    pub target_provider_id: String,
    pub source_provider_ids: Vec<String>,
    pub mode: Option<String>,
    pub conflict_policy: Option<String>,
    pub since_ts: Option<i64>,
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncResult {
    pub total_scanned: usize,
    pub imported: usize,
    pub skipped: usize,
    pub conflicts: usize,
    pub failed: usize,
    pub warnings: Vec<String>,
}

fn compute_sync_preview(
    sessions: &[session_manager::SessionMeta],
    request: &SessionSyncRequest,
) -> SessionSyncResult {
    let sources: HashSet<&str> = request
        .source_provider_ids
        .iter()
        .map(String::as_str)
        .collect();
    let target_session_ids: HashSet<&str> = sessions
        .iter()
        .filter(|s| s.provider_id == request.target_provider_id)
        .map(|s| s.session_id.as_str())
        .collect();

    let mut total_scanned = 0usize;
    let mut conflicts = 0usize;
    let mut imported = 0usize;
    let mut skipped = 0usize;

    for session in sessions
        .iter()
        .filter(|s| sources.contains(s.provider_id.as_str()))
    {
        if let Some(since_ts) = request.since_ts {
            let ts = session.last_active_at.or(session.created_at).unwrap_or(0);
            if ts < since_ts {
                skipped += 1;
                continue;
            }
        }

        total_scanned += 1;
        if target_session_ids.contains(session.session_id.as_str()) {
            conflicts += 1;
            match request.conflict_policy.as_deref() {
                Some("overwrite") | Some("duplicate_new_id") => imported += 1,
                _ => {}
            }
        } else {
            imported += 1;
        }
    }

    SessionSyncResult {
        total_scanned,
        imported,
        skipped,
        conflicts,
        failed: 0,
        warnings: vec![],
    }
}

fn validate_and_normalize_request(
    request: &SessionSyncRequest,
) -> Result<(Vec<String>, Vec<String>), String> {
    if request.target_provider_id.trim().is_empty() {
        return Err("targetProviderId is required".to_string());
    }
    if request.source_provider_ids.is_empty() {
        return Err("sourceProviderIds is required".to_string());
    }
    if !SUPPORTED_PROVIDERS.contains(&request.target_provider_id.as_str()) {
        return Err(format!(
            "Unsupported target provider: {}",
            request.target_provider_id
        ));
    }
    if let Some(mode) = request.mode.as_deref() {
        if mode != "metadata_only" && mode != "full_messages" {
            return Err(format!("Unsupported mode: {mode}"));
        }
    }
    if let Some(policy) = request.conflict_policy.as_deref() {
        if policy != "keep_target" && policy != "overwrite" && policy != "duplicate_new_id" {
            return Err(format!("Unsupported conflictPolicy: {policy}"));
        }
    }

    let mut valid_sources: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    for source in &request.source_provider_ids {
        if source == &request.target_provider_id {
            warnings.push(format!(
                "Skipped source provider identical to target: {source}"
            ));
            continue;
        }
        if SUPPORTED_PROVIDERS.contains(&source.as_str()) {
            valid_sources.push(source.clone());
        } else {
            warnings.push(format!("Skipped unsupported source provider: {source}"));
        }
    }

    if valid_sources.is_empty() {
        return Err("No valid source providers after validation".to_string());
    }

    Ok((valid_sources, warnings))
}

#[tauri::command]
pub async fn list_sessions() -> Result<Vec<session_manager::SessionMeta>, String> {
    let sessions = tauri::async_runtime::spawn_blocking(session_manager::scan_sessions)
        .await
        .map_err(|e| format!("Failed to scan sessions: {e}"))?;
    Ok(sessions)
}

#[tauri::command]
pub async fn get_session_messages(
    providerId: String,
    sourcePath: String,
) -> Result<Vec<session_manager::SessionMessage>, String> {
    let provider_id = providerId.clone();
    let source_path = sourcePath.clone();
    tauri::async_runtime::spawn_blocking(move || {
        session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to load session messages: {e}"))?
}

#[tauri::command]
pub async fn launch_session_terminal(
    command: String,
    cwd: Option<String>,
    custom_config: Option<String>,
) -> Result<bool, String> {
    let command = command.clone();
    let cwd = cwd.clone();
    let custom_config = custom_config.clone();

    // Read preferred terminal from global settings
    let preferred = crate::settings::get_preferred_terminal();
    // Map global setting terminal names to session terminal names
    // Global uses "iterm2", session terminal uses "iterm"
    let target = match preferred.as_deref() {
        Some("iterm2") => "iterm".to_string(),
        Some(t) => t.to_string(),
        None => "terminal".to_string(), // Default to Terminal.app on macOS
    };

    tauri::async_runtime::spawn_blocking(move || {
        session_manager::terminal::launch_terminal(
            &target,
            &command,
            cwd.as_deref(),
            custom_config.as_deref(),
        )
    })
    .await
    .map_err(|e| format!("Failed to launch terminal: {e}"))??;

    Ok(true)
}

#[tauri::command]
pub async fn delete_session(
    providerId: String,
    sessionId: String,
    sourcePath: String,
) -> Result<bool, String> {
    let provider_id = providerId.clone();
    let session_id = sessionId.clone();
    let source_path = sourcePath.clone();

    tauri::async_runtime::spawn_blocking(move || {
        session_manager::delete_session(&provider_id, &session_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to delete session: {e}"))?
}

#[tauri::command]
pub async fn delete_sessions(
    items: Vec<session_manager::DeleteSessionRequest>,
) -> Result<Vec<session_manager::DeleteSessionOutcome>, String> {
    tauri::async_runtime::spawn_blocking(move || session_manager::delete_sessions(&items))
        .await
        .map_err(|e| format!("Failed to delete sessions: {e}"))
}

#[tauri::command]
pub async fn preview_session_sync(
    request: SessionSyncRequest,
) -> Result<SessionSyncResult, String> {
    let (valid_sources, pre_warnings) = validate_and_normalize_request(&request)?;
    let mut normalized = request.clone();
    normalized.source_provider_ids = valid_sources;

    tauri::async_runtime::spawn_blocking(move || {
        let sessions = session_manager::scan_sessions();
        let mut result = compute_sync_preview(&sessions, &normalized);
        result.warnings.extend(pre_warnings);
        Ok(result)
    })
    .await
    .map_err(|e| format!("Failed to preview session sync: {e}"))?
}

#[tauri::command]
pub async fn sync_sessions_to_provider(
    request: SessionSyncRequest,
) -> Result<SessionSyncResult, String> {
    if request.dry_run.unwrap_or(false) {
        let mut result = preview_session_sync(request).await?;
        result
            .warnings
            .push("dryRun=true: returning preview only".to_string());
        return Ok(result);
    }

    Err("sync execution is not implemented yet; pass dryRun=true for preview results".to_string())
}

#[cfg(test)]
mod tests {
    use super::{compute_sync_preview, validate_and_normalize_request, SessionSyncRequest};
    use crate::session_manager::SessionMeta;

    fn make_session(provider_id: &str, session_id: &str, ts: i64) -> SessionMeta {
        SessionMeta {
            provider_id: provider_id.to_string(),
            session_id: session_id.to_string(),
            title: None,
            summary: None,
            project_dir: None,
            created_at: Some(ts),
            last_active_at: Some(ts),
            source_path: None,
            resume_command: None,
        }
    }

    #[test]
    fn preview_counts_conflicts_and_imports() {
        let sessions = vec![
            make_session("codex", "s1", 100),
            make_session("codex", "s2", 200),
            make_session("claude", "s2", 300), // conflict in target
            make_session("claude", "s3", 400),
        ];

        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec!["codex".to_string()],
            mode: None,
            conflict_policy: None,
            since_ts: None,
            dry_run: None,
        };

        let result = compute_sync_preview(&sessions, &request);
        assert_eq!(result.total_scanned, 2);
        assert_eq!(result.conflicts, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 0);
    }

    #[test]
    fn preview_applies_since_filter() {
        let sessions = vec![
            make_session("codex", "old", 100),
            make_session("codex", "new", 500),
            make_session("claude", "existing", 300),
        ];

        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec!["codex".to_string()],
            mode: None,
            conflict_policy: None,
            since_ts: Some(300),
            dry_run: None,
        };

        let result = compute_sync_preview(&sessions, &request);
        assert_eq!(result.total_scanned, 1);
        assert_eq!(result.imported, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(result.conflicts, 0);
    }

    #[test]
    fn preview_conflict_policy_overwrite_counts_as_importable() {
        let sessions = vec![
            make_session("codex", "s1", 100),
            make_session("claude", "s1", 200), // conflict in target
        ];

        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec!["codex".to_string()],
            mode: None,
            conflict_policy: Some("overwrite".to_string()),
            since_ts: None,
            dry_run: None,
        };

        let result = compute_sync_preview(&sessions, &request);
        assert_eq!(result.total_scanned, 1);
        assert_eq!(result.conflicts, 1);
        assert_eq!(result.imported, 1);
    }

    #[test]
    fn validation_filters_same_target_and_unknown_sources() {
        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec![
                "claude".to_string(),
                "codex".to_string(),
                "unknown".to_string(),
            ],
            mode: Some("metadata_only".to_string()),
            conflict_policy: Some("keep_target".to_string()),
            since_ts: None,
            dry_run: Some(true),
        };

        let (sources, warnings) = validate_and_normalize_request(&request).expect("valid request");
        assert_eq!(sources, vec!["codex".to_string()]);
        assert_eq!(warnings.len(), 2);
    }
}
