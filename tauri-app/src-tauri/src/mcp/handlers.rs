//! MCP tool handlers for the AI Scribe worker agent.
//!
//! Implements the standard worker MCP tools plus scribe-specific tools.

use crate::config;
use crate::session::{SessionManager, SessionState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::types::ToolResult;

/// Application start time for uptime calculation
static APP_START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Initialize the app start time (call once at startup)
pub fn init_start_time() {
    APP_START_TIME.get_or_init(Instant::now);
}

/// Get uptime in seconds
fn get_uptime_seconds() -> u64 {
    APP_START_TIME
        .get()
        .map(|t| t.elapsed().as_secs())
        .unwrap_or(0)
}

// ============================================================================
// Agent Identity
// ============================================================================

#[derive(Debug, Serialize)]
struct AgentIdentityResponse {
    agent_id: String,
    agent_name: String,
    version: String,
    machine: String,
    mcp_port: u16,
    uptime_seconds: u64,
    started_at: String,
    capabilities: Vec<String>,
}

/// Handle the agent_identity tool
pub fn handle_agent_identity() -> ToolResult {
    let started_at = chrono::Utc::now()
        .checked_sub_signed(chrono::Duration::seconds(get_uptime_seconds() as i64))
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let response = AgentIdentityResponse {
        agent_id: "clinic-scribe".to_string(),
        agent_name: "AI Scribe Agent".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        machine: hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string()),
        mcp_port: 7101,
        uptime_seconds: get_uptime_seconds(),
        started_at,
        capabilities: vec![
            "realtime_transcription_display".to_string(),
            "soap_generation".to_string(),
            "emr_integration".to_string(),
            "template_management".to_string(),
        ],
    };

    ToolResult::success(&response)
}

// ============================================================================
// Health Check
// ============================================================================

#[derive(Debug, Serialize)]
struct HealthCheckResponse {
    healthy: bool,
    timestamp: String,
    checks: HealthChecks,
}

#[derive(Debug, Serialize)]
struct HealthChecks {
    services_running: bool,
    disk_space_ok: bool,
    memory_ok: bool,
    dependencies_ok: bool,
}

/// Handle the health_check tool
pub fn handle_health_check(session_manager: &Arc<Mutex<SessionManager>>) -> ToolResult {
    // Check if session manager is accessible (services running)
    let services_running = session_manager.lock().is_ok();

    // Check disk space (models directory)
    let disk_space_ok = check_disk_space();

    // For now, assume memory is OK (could add actual check later)
    let memory_ok = true;

    // Check dependencies by loading config and verifying URLs are configured
    let dependencies_ok = check_dependencies();

    let healthy = services_running && disk_space_ok && memory_ok && dependencies_ok;

    let response = HealthCheckResponse {
        healthy,
        timestamp: chrono::Utc::now().to_rfc3339(),
        checks: HealthChecks {
            services_running,
            disk_space_ok,
            memory_ok,
            dependencies_ok,
        },
    };

    ToolResult::success(&response)
}

fn check_disk_space() -> bool {
    // Check if we have at least 100MB free in the models directory
    if let Some(home) = dirs::home_dir() {
        let models_dir = home.join(".transcriptionapp").join("models");
        if models_dir.exists() {
            // Simple check: try to get available space
            // For now, just check if the directory is writable
            let test_file = models_dir.join(".disk_check");
            if fs::write(&test_file, "test").is_ok() {
                let _ = fs::remove_file(&test_file);
                return true;
            }
        } else {
            // Models directory doesn't exist yet, that's OK
            return true;
        }
    }
    false
}

fn check_dependencies() -> bool {
    // Load config and check if required services are configured
    match config::Config::load() {
        Ok(cfg) => {
            // Check Whisper (local or remote must be configured)
            let whisper_ok = if cfg.whisper_mode == "remote" {
                !cfg.whisper_server_url.is_empty()
            } else {
                true // Local mode, model will be downloaded
            };

            // Check LLM router
            let llm_ok = !cfg.llm_router_url.is_empty();

            whisper_ok && llm_ok
        }
        Err(_) => false,
    }
}

// ============================================================================
// Get Status
// ============================================================================

#[derive(Debug, Serialize)]
struct StatusResponse {
    status: String,
    services: Vec<ServiceStatus>,
    last_activity: String,
    active_tasks: u32,
    queued_tasks: u32,
    error_count_last_hour: u32,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ServiceStatus {
    name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

/// Handle the get_status tool
pub fn handle_get_status(session_manager: &Arc<Mutex<SessionManager>>) -> ToolResult {
    let mut services = Vec::new();
    let mut warnings = Vec::new();
    let mut overall_status = "healthy";

    // Check session/UI service
    let (session_status, session_details, active_tasks) = match session_manager.lock() {
        Ok(session) => {
            let state = session.state();
            let status_str = match state {
                SessionState::Idle => "idle",
                SessionState::Preparing => "preparing",
                SessionState::Recording => "recording",
                SessionState::Stopping => "stopping",
                SessionState::Completed => "completed",
                SessionState::Error => "error",
            };

            let active = if *state == SessionState::Recording {
                1
            } else {
                0
            };

            let details = if *state == SessionState::Error {
                session.status().error_message
            } else {
                None
            };

            (status_str, details, active)
        }
        Err(_) => {
            overall_status = "error";
            ("error", Some("Failed to access session state".to_string()), 0)
        }
    };

    services.push(ServiceStatus {
        name: "scribe-ui".to_string(),
        status: if session_status == "error" {
            "error".to_string()
        } else {
            "running".to_string()
        },
        details: session_details,
    });

    // Check config/dependencies
    match config::Config::load() {
        Ok(cfg) => {
            // Whisper service status
            let whisper_status = if cfg.whisper_mode == "remote" {
                ServiceStatus {
                    name: "whisper-remote".to_string(),
                    status: "configured".to_string(),
                    details: Some(cfg.whisper_server_url.clone()),
                }
            } else {
                ServiceStatus {
                    name: "whisper-local".to_string(),
                    status: "configured".to_string(),
                    details: Some(cfg.whisper_model.clone()),
                }
            };
            services.push(whisper_status);

            // LLM router status
            services.push(ServiceStatus {
                name: "llm-router".to_string(),
                status: if cfg.llm_router_url.is_empty() {
                    warnings.push("LLM router URL not configured".to_string());
                    overall_status = "degraded";
                    "not_configured".to_string()
                } else {
                    "configured".to_string()
                },
                details: if cfg.llm_router_url.is_empty() {
                    None
                } else {
                    Some(cfg.llm_router_url.clone())
                },
            });

            // Medplum status
            services.push(ServiceStatus {
                name: "medplum".to_string(),
                status: if cfg.medplum_server_url.is_empty() {
                    "not_configured".to_string()
                } else {
                    "configured".to_string()
                },
                details: if cfg.medplum_server_url.is_empty() {
                    None
                } else {
                    Some(cfg.medplum_server_url.clone())
                },
            });
        }
        Err(e) => {
            warnings.push(format!("Failed to load config: {}", e));
            overall_status = "degraded";
        }
    }

    let response = StatusResponse {
        status: overall_status.to_string(),
        services,
        last_activity: chrono::Utc::now().to_rfc3339(),
        active_tasks,
        queued_tasks: 0,
        error_count_last_hour: 0, // TODO: Track errors
        warnings,
    };

    ToolResult::success(&response)
}

// ============================================================================
// Get Logs
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct GetLogsParams {
    #[serde(default = "default_lines")]
    pub lines: usize,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
}

fn default_lines() -> usize {
    50
}

#[derive(Debug, Serialize)]
struct LogEntry {
    timestamp: String,
    level: String,
    service: String,
    message: String,
}

#[derive(Debug, Serialize)]
struct GetLogsResponse {
    entries: Vec<LogEntry>,
    total_matching: usize,
    truncated: bool,
}

/// Handle the get_logs tool
pub fn handle_get_logs(params: GetLogsParams) -> ToolResult {
    let max_lines = params.lines.min(500); // Cap at 500

    // Get log directory
    let log_dir = match dirs::home_dir() {
        Some(home) => home.join(".transcriptionapp").join("logs"),
        None => return ToolResult::error("Could not determine home directory"),
    };

    if !log_dir.exists() {
        return ToolResult::success(&GetLogsResponse {
            entries: vec![],
            total_matching: 0,
            truncated: false,
        });
    }

    // Find log files (sorted by name, most recent first)
    let mut log_files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = fs::read_dir(&log_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "log").unwrap_or(false)
                || path
                    .file_name()
                    .map(|n| n.to_string_lossy().starts_with("activity.log"))
                    .unwrap_or(false)
            {
                log_files.push(path);
            }
        }
    }
    log_files.sort();
    log_files.reverse(); // Most recent first

    // Parse log entries
    let mut entries: Vec<LogEntry> = Vec::new();
    let mut total_matching = 0;

    // Parse "since" timestamp if provided
    let since_time = params.since.as_ref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|t| t.with_timezone(&chrono::Utc))
    });

    for log_file in log_files {
        if entries.len() >= max_lines {
            break;
        }

        let file = match fs::File::open(&log_file) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let reader = BufReader::new(file);
        let mut file_entries: Vec<LogEntry> = Vec::new();

        for line in reader.lines().flatten() {
            // Try to parse as JSON log entry
            if let Ok(json) = serde_json::from_str::<Value>(&line) {
                let timestamp = json
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let level = json
                    .get("level")
                    .and_then(|v| v.as_str())
                    .unwrap_or("info")
                    .to_string()
                    .to_uppercase();

                let target = json
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Extract service name from target (e.g., "transcription_app_lib::session" -> "session")
                let service = target.split("::").last().unwrap_or(&target).to_string();

                let message = json
                    .get("fields")
                    .and_then(|f| f.get("message"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                // Apply filters
                let mut matches = true;

                // Level filter
                if let Some(ref level_filter) = params.level {
                    if level_filter != "all" {
                        let filter_priority = level_priority(level_filter);
                        let entry_priority = level_priority(&level);
                        matches = matches && entry_priority >= filter_priority;
                    }
                }

                // Service filter
                if let Some(ref service_filter) = params.service {
                    matches = matches
                        && service
                            .to_lowercase()
                            .contains(&service_filter.to_lowercase());
                }

                // Since filter
                if let Some(ref since) = since_time {
                    if let Ok(entry_time) = chrono::DateTime::parse_from_rfc3339(&timestamp) {
                        matches = matches && entry_time.with_timezone(&chrono::Utc) >= *since;
                    }
                }

                // Search filter
                if let Some(ref search) = params.search {
                    matches = matches
                        && (message.to_lowercase().contains(&search.to_lowercase())
                            || target.to_lowercase().contains(&search.to_lowercase()));
                }

                if matches {
                    total_matching += 1;
                    file_entries.push(LogEntry {
                        timestamp,
                        level,
                        service,
                        message,
                    });
                }
            }
        }

        // Reverse to get chronological order within file, then take from end
        file_entries.reverse();
        for entry in file_entries.into_iter().rev() {
            if entries.len() >= max_lines {
                break;
            }
            entries.push(entry);
        }
    }

    // Reverse to show newest first
    entries.reverse();
    entries.truncate(max_lines);

    let truncated = total_matching > entries.len();

    ToolResult::success(&GetLogsResponse {
        entries,
        total_matching,
        truncated,
    })
}

fn level_priority(level: &str) -> u8 {
    match level.to_lowercase().as_str() {
        "error" => 4,
        "warn" | "warning" => 3,
        "info" => 2,
        "debug" => 1,
        "trace" => 0,
        _ => 2,
    }
}

// ============================================================================
// Tools List
// ============================================================================

use super::types::{ToolDefinition, ToolsListResult};

/// Return the list of available tools
pub fn get_tools_list() -> ToolsListResult {
    ToolsListResult {
        tools: vec![
            ToolDefinition {
                name: "agent_identity".to_string(),
                description: "Returns the agent's identity and basic information".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "health_check".to_string(),
                description: "Quick health check endpoint for monitoring".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "get_status".to_string(),
                description: "Returns the current operational status of the agent and its managed services".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "get_logs".to_string(),
                description: "Retrieves recent log entries from the agent's managed services".to_string(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "lines": {
                            "type": "integer",
                            "description": "Number of lines (default: 50, max: 500)",
                            "default": 50
                        },
                        "level": {
                            "type": "string",
                            "description": "Filter by level: all, error, warn, info, debug",
                            "enum": ["all", "error", "warn", "info", "debug"]
                        },
                        "service": {
                            "type": "string",
                            "description": "Filter by service name"
                        },
                        "since": {
                            "type": "string",
                            "description": "ISO timestamp to filter from"
                        },
                        "search": {
                            "type": "string",
                            "description": "Text search filter"
                        }
                    },
                    "required": []
                }),
            },
        ],
    }
}
