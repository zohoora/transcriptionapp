//! MCP (Model Context Protocol) server for the AI Scribe worker agent.
//!
//! This module implements a JSON-RPC 2.0 server on port 7101 that exposes
//! monitoring tools for the IT Admin Coordinator.
//!
//! ## Tools
//!
//! ### V1 (Monitor Only)
//! - `agent_identity` - Returns agent identity and capabilities
//! - `health_check` - Quick health check for monitoring
//! - `get_status` - Detailed operational status
//! - `get_logs` - Retrieve recent log entries
//!
//! ### Future (V2+)
//! - `get_active_encounters` - List active encounters
//! - `get_templates` - List SOAP templates
//!
//! ## Usage
//!
//! The MCP server is started automatically when the Tauri app launches.
//! It runs on a separate async task and shares state with the main app.
//!
//! ```ignore
//! // From lib.rs setup
//! let session_manager = Arc::new(Mutex::new(SessionManager::new()));
//!
//! // Spawn MCP server
//! let mcp_session = session_manager.clone();
//! tokio::spawn(async move {
//!     mcp::start_mcp_server(mcp_session).await;
//! });
//! ```
//!
//! ## Protocol
//!
//! The server accepts JSON-RPC 2.0 requests at `POST /mcp`:
//!
//! ```json
//! {
//!     "jsonrpc": "2.0",
//!     "id": 1,
//!     "method": "tools/call",
//!     "params": {
//!         "name": "health_check",
//!         "arguments": {}
//!     }
//! }
//! ```

mod handlers;
mod server;
mod types;

pub use server::start_mcp_server;
