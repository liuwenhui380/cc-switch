#![allow(non_snake_case)]

use crate::session_manager;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncCapabilities {
    pub execution_supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub supported_target_providers: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionSyncReport {
    target_provider_id: String,
    source_provider_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conflict_policy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since_ts: Option<i64>,
    dry_run: bool,
    created_at_ms: i64,
    total_scanned: usize,
    imported: usize,
    skipped: usize,
    conflicts: usize,
    failed: usize,
    warnings: Vec<String>,
}

fn persist_sync_report(
    request: &SessionSyncRequest,
    result: &SessionSyncResult,
) -> Result<(), String> {
    let app_dir = crate::config::get_app_config_dir();
    let report_dir = app_dir.join("session-sync-reports");
    fs::create_dir_all(&report_dir).map_err(|e| {
        format!(
            "Failed to create report directory {}: {e}",
            report_dir.display()
        )
    })?;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Failed to get timestamp: {e}"))?
        .as_millis() as i64;

    let report = SessionSyncReport {
        target_provider_id: request.target_provider_id.clone(),
        source_provider_ids: request.source_provider_ids.clone(),
        mode: request.mode.clone(),
        conflict_policy: request.conflict_policy.clone(),
        since_ts: request.since_ts,
        dry_run: request.dry_run.unwrap_or(false),
        created_at_ms: ts,
        total_scanned: result.total_scanned,
        imported: result.imported,
        skipped: result.skipped,
        conflicts: result.conflicts,
        failed: result.failed,
        warnings: result.warnings.clone(),
    };

    let latest = report_dir.join("latest.json");
    let stamped = report_dir.join(format!(
        "report-{}-{}.json",
        request.target_provider_id, ts
    ));
    let content = serde_json::to_string_pretty(&report)
        .map_err(|e| format!("serialize report failed: {e}"))?;
    fs::write(&latest, &content).map_err(|e| format!("write latest report failed: {e}"))?;
    fs::write(&stamped, &content).map_err(|e| format!("write stamped report failed: {e}"))?;
    Ok(())
}

fn codex_sessions_root() -> PathBuf {
    crate::codex_config::get_codex_config_dir().join("sessions")
}

fn claude_sessions_root() -> PathBuf {
    crate::config::get_claude_config_dir().join("projects")
}

fn gemini_sessions_root() -> PathBuf {
    crate::gemini_config::get_gemini_dir().join("tmp")
}

fn make_sync_session_id(seed: i64, index: usize) -> String {
    format!("sync-{seed:016x}-{index:04x}")
}

fn write_codex_import_session(
    session_id: &str,
    messages: &[session_manager::SessionMessage],
    project_dir: Option<&str>,
    timestamp_ms: i64,
) -> Result<PathBuf, String> {
    let root = codex_sessions_root();
    fs::create_dir_all(&root).map_err(|e| format!("Failed to create Codex session root: {e}"))?;
    let file_path = root.join(format!("{session_id}.jsonl"));

    let mut lines = Vec::new();
    lines.push(
        json!({
            "type": "session_meta",
            "timestamp": timestamp_ms,
            "payload": {
                "id": session_id,
                "cwd": project_dir.unwrap_or("."),
                "timestamp": timestamp_ms
            }
        })
        .to_string(),
    );

    for message in messages {
        lines.push(
            json!({
                "type": "response_item",
                "timestamp": message.ts.unwrap_or(timestamp_ms),
                "payload": {
                    "type": "message",
                    "role": message.role,
                    "content": [{ "type": "text", "text": message.content }]
                }
            })
            .to_string(),
        );
    }

    fs::write(&file_path, lines.join("\n") + "\n")
        .map_err(|e| format!("Failed to write imported Codex session: {e}"))?;
    Ok(file_path)
}

fn write_claude_import_session(
    session_id: &str,
    messages: &[session_manager::SessionMessage],
    project_dir: Option<&str>,
    timestamp_ms: i64,
) -> Result<PathBuf, String> {
    let root = claude_sessions_root();
    fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create Claude session root: {e}"))?;
    let file_path = root.join(format!("{session_id}.jsonl"));

    let mut lines = Vec::new();
    for message in messages {
        lines.push(
            json!({
                "sessionId": session_id,
                "cwd": project_dir.unwrap_or("."),
                "timestamp": message.ts.unwrap_or(timestamp_ms),
                "type": message.role,
                "message": {
                    "role": message.role,
                    "content": [{ "type": "text", "text": message.content }]
                }
            })
            .to_string(),
        );
    }

    fs::write(&file_path, lines.join("\n") + "\n")
        .map_err(|e| format!("Failed to write imported Claude session: {e}"))?;
    Ok(file_path)
}

fn write_gemini_import_session(
    session_id: &str,
    messages: &[session_manager::SessionMessage],
    timestamp_ms: i64,
) -> Result<PathBuf, String> {
    let root = gemini_sessions_root();
    fs::create_dir_all(&root)
        .map_err(|e| format!("Failed to create Gemini session root: {e}"))?;
    let file_path = root.join(format!("{session_id}.json"));

    let body = json!({
        "sessionId": session_id,
        "startTime": timestamp_ms,
        "lastUpdated": timestamp_ms,
        "messages": messages.iter().map(|m| {
            json!({
                "id": format!("{}-{}", session_id, m.ts.unwrap_or(timestamp_ms)),
                "timestamp": m.ts.unwrap_or(timestamp_ms),
                "type": if m.role == "user" { "user" } else { "model" },
                "content": m.content
            })
        }).collect::<Vec<_>>()
    });

    fs::write(
        &file_path,
        serde_json::to_string_pretty(&body).map_err(|e| format!("serialize failed: {e}"))?,
    )
    .map_err(|e| format!("Failed to write imported Gemini session: {e}"))?;
    Ok(file_path)
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
    let mut seen_sources: HashSet<String> = HashSet::new();
    let mut warnings: Vec<String> = Vec::new();
    for source in &request.source_provider_ids {
        if source == &request.target_provider_id {
            warnings.push(format!(
                "Skipped source provider identical to target: {source}"
            ));
            continue;
        }
        if SUPPORTED_PROVIDERS.contains(&source.as_str()) {
            if seen_sources.insert(source.clone()) {
                valid_sources.push(source.clone());
            } else {
                warnings.push(format!("Skipped duplicate source provider: {source}"));
            }
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
    if request.dry_run == Some(false) {
        return Err(
            "preview_session_sync only supports dryRun=true (or omitted) requests".to_string(),
        );
    }

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
    let report_request = request.clone();
    let dry_run = request.dry_run.unwrap_or(false);
    let target_provider_id = request.target_provider_id.clone();
    let mut result = preview_session_sync(request).await?;

    if dry_run {
        result
            .warnings
            .push("dryRun=true: returning preview only".to_string());
        match persist_sync_report(&report_request, &result) {
            Ok(()) => result
                .warnings
                .push("sync summary report persisted under session-sync-reports".to_string()),
            Err(error) => result
                .warnings
                .push(format!("failed to persist sync summary report: {error}")),
        }
        return Ok(result);
    }

    if target_provider_id != "codex"
        && target_provider_id != "claude"
        && target_provider_id != "gemini"
    {
        let mut rejected = result;
        let would_import = rejected.imported;
        rejected.imported = 0;
        rejected.failed = would_import;
        rejected.warnings.push(
            "sync execution currently supports target providers 'codex', 'claude' and 'gemini' only"
                .to_string(),
        );
        let persistence_note = match persist_sync_report(&report_request, &rejected) {
            Ok(()) => "rejection report persisted under session-sync-reports".to_string(),
            Err(error) => format!("failed to persist rejection report: {error}"),
        };
        return Err(format!(
            "sync execution currently supports target providers 'codex', 'claude' and 'gemini' only ({})",
            persistence_note
        ));
    }

    let (valid_sources, _) = validate_and_normalize_request(&report_request)?;
    let sessions = session_manager::scan_sessions();
    let target_ids: HashSet<&str> = sessions
        .iter()
        .filter(|s| s.provider_id == target_provider_id)
        .map(|s| s.session_id.as_str())
        .collect();

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Failed to get timestamp: {e}"))?
        .as_millis() as i64;

    let mut imported = 0usize;
    let mut failed = 0usize;
    let mut warnings = result.warnings.clone();
    let mut written_files: Vec<PathBuf> = Vec::new();
    for (index, session) in sessions
        .iter()
        .filter(|s| valid_sources.contains(&s.provider_id))
        .enumerate()
    {
        if let Some(since_ts) = report_request.since_ts {
            let ts = session.last_active_at.or(session.created_at).unwrap_or(0);
            if ts < since_ts {
                continue;
            }
        }

        let is_conflict = target_ids.contains(session.session_id.as_str());
        let policy = report_request
            .conflict_policy
            .as_deref()
            .unwrap_or("keep_target");
        if is_conflict && policy == "keep_target" {
            continue;
        }
        let Some(source_path) = session.source_path.as_deref() else {
            failed += 1;
            continue;
        };
        let messages = match session_manager::load_messages(&session.provider_id, source_path) {
            Ok(v) if !v.is_empty() => v,
            Ok(_) => {
                failed += 1;
                continue;
            }
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let new_id = make_sync_session_id(now_ms, index);
        let write_result = if target_provider_id == "codex" {
            write_codex_import_session(&new_id, &messages, session.project_dir.as_deref(), now_ms)
        } else if target_provider_id == "claude" {
            write_claude_import_session(
                &new_id,
                &messages,
                session.project_dir.as_deref(),
                now_ms,
            )
        } else {
            write_gemini_import_session(&new_id, &messages, now_ms)
        };
        match write_result {
            Ok(path) => {
                imported += 1;
                written_files.push(path);
            }
            Err(e) => {
                failed += 1;
                warnings.push(format!("failed to import session {}: {e}", session.session_id));
            }
        }
    }

    if failed > 0 && !written_files.is_empty() {
        let mut rollback_failed = 0usize;
        for path in written_files {
            if fs::remove_file(&path).is_err() {
                rollback_failed += 1;
            }
        }
        warnings.push(format!(
            "partial sync detected; rolled back imported files (rollback failures: {rollback_failed})"
        ));
        imported = 0;
    }

    result.imported = imported;
    result.failed = failed;
    result.warnings = warnings;
    persist_sync_report(&report_request, &result).ok();
    Ok(result)
}

#[tauri::command]
pub async fn get_session_sync_capabilities(
    target_provider_id: Option<String>,
) -> Result<SessionSyncCapabilities, String> {
    let supported_target_providers = vec![
        "codex".to_string(),
        "claude".to_string(),
        "gemini".to_string(),
    ];
    let target_supported = target_provider_id
        .as_deref()
        .map(|target| supported_target_providers.iter().any(|p| p == target))
        .unwrap_or(true);

    Ok(SessionSyncCapabilities {
        execution_supported: target_supported,
        reason: Some(if target_supported {
            "Sync execution is currently available for target providers 'codex', 'claude' and 'gemini' (MVP mode).".to_string()
        } else {
            format!(
                "Sync execution is not available for target provider '{}'; supported targets: codex, claude, gemini.",
                target_provider_id.unwrap_or_default()
            )
        }),
        supported_target_providers,
    })
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

    #[test]
    fn validation_deduplicates_duplicate_sources() {
        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec![
                "codex".to_string(),
                "codex".to_string(),
                "gemini".to_string(),
            ],
            mode: None,
            conflict_policy: None,
            since_ts: None,
            dry_run: None,
        };

        let (sources, warnings) = validate_and_normalize_request(&request).expect("valid request");
        assert_eq!(sources, vec!["codex".to_string(), "gemini".to_string()]);
        assert!(warnings
            .iter()
            .any(|w| w.contains("Skipped duplicate source provider: codex")));
    }

    #[tokio::test]
    async fn sync_rejects_non_dry_run_until_execution_is_implemented() {
        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec!["codex".to_string()],
            mode: Some("metadata_only".to_string()),
            conflict_policy: Some("keep_target".to_string()),
            since_ts: None,
            dry_run: Some(false),
        };

        let error = sync_sessions_to_provider(request)
            .await
            .expect_err("expected sync to reject while execution path is unimplemented");
        assert!(
            error.contains("sync execution is not implemented yet"),
            "should report explicit non-dry-run unsupported status"
        );
    }

    #[tokio::test]
    async fn preview_rejects_explicit_non_dry_run_requests() {
        let request = SessionSyncRequest {
            target_provider_id: "claude".to_string(),
            source_provider_ids: vec!["codex".to_string()],
            mode: Some("metadata_only".to_string()),
            conflict_policy: Some("keep_target".to_string()),
            since_ts: None,
            dry_run: Some(false),
        };

        let error = preview_session_sync(request)
            .await
            .expect_err("expected preview command to reject dryRun=false");
        assert!(error.contains("only supports dryRun=true"));
    }
}
