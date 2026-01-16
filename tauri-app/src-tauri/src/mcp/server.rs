//! MCP HTTP server for the AI Scribe worker agent.
//!
//! Provides a JSON-RPC 2.0 endpoint on port 7101 for the IT Admin Coordinator.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

use crate::session::SessionManager;

use super::handlers::{
    self, get_tools_list, handle_agent_identity, handle_get_logs, handle_get_status,
    handle_health_check, GetLogsParams,
};
use super::types::{JsonRpcRequest, JsonRpcResponse, ToolCallParams, ToolResult};

/// Shared state for the MCP server
#[derive(Clone)]
pub struct McpState {
    pub session_manager: Arc<Mutex<SessionManager>>,
}

/// Start the MCP server on port 7101
pub async fn start_mcp_server(session_manager: Arc<Mutex<SessionManager>>) {
    // Initialize start time for uptime tracking
    handlers::init_start_time();

    let state = McpState { session_manager };

    // Build router with CORS for cross-origin requests
    let app = Router::new()
        .route("/mcp", post(mcp_handler))
        .route("/health", get(health_endpoint))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], 7101));
    info!("MCP server starting on {}", addr);

    // Use axum's serve with graceful shutdown
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind MCP server to {}: {}", addr, e);
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        error!("MCP server error: {}", e);
    }
}

/// Simple health endpoint for quick checks (non-MCP)
async fn health_endpoint(State(state): State<McpState>) -> Json<Value> {
    let healthy = state.session_manager.lock().is_ok();
    Json(serde_json::json!({
        "healthy": healthy,
        "agent_id": "clinic-scribe",
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))
}

/// Main MCP JSON-RPC handler
async fn mcp_handler(
    State(state): State<McpState>,
    Json(req): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    // Validate JSON-RPC version
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                req.id,
                -32600,
                "Invalid JSON-RPC version",
            )),
        );
    }

    let response = match req.method.as_str() {
        // MCP standard methods
        "tools/list" => {
            let result = get_tools_list();
            JsonRpcResponse::success(req.id, serde_json::to_value(result).unwrap())
        }

        "tools/call" => handle_tool_call(&state, req.id.clone(), req.params),

        // Unknown method
        _ => {
            warn!("Unknown MCP method: {}", req.method);
            JsonRpcResponse::method_not_found(req.id, &req.method)
        }
    };

    (StatusCode::OK, Json(response))
}

/// Handle a tools/call request
fn handle_tool_call(state: &McpState, id: Value, params: Option<Value>) -> JsonRpcResponse {
    // Parse tool call params
    let tool_params: ToolCallParams = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(tp) => tp,
            Err(e) => {
                return JsonRpcResponse::invalid_params(id, format!("Invalid params: {}", e));
            }
        },
        None => {
            return JsonRpcResponse::invalid_params(id, "Missing params");
        }
    };

    info!("MCP tool call: {}", tool_params.name);

    // Dispatch to tool handler
    let result: ToolResult = match tool_params.name.as_str() {
        "agent_identity" => handle_agent_identity(),

        "health_check" => handle_health_check(&state.session_manager),

        "get_status" => handle_get_status(&state.session_manager),

        "get_logs" => {
            // Parse get_logs arguments
            let log_params: GetLogsParams = serde_json::from_value(tool_params.arguments)
                .unwrap_or(GetLogsParams {
                    lines: 50,
                    level: None,
                    service: None,
                    since: None,
                    search: None,
                });
            handle_get_logs(log_params)
        }

        _ => {
            warn!("Unknown tool: {}", tool_params.name);
            ToolResult::error(format!("Unknown tool: {}", tool_params.name))
        }
    };

    // Wrap result in JSON-RPC response
    JsonRpcResponse::success(id, serde_json::to_value(result).unwrap())
}
