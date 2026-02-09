# AI Scribe MCP Server Documentation

## Overview

The AI Scribe application (`clinic-scribe`) exposes an MCP (Model Context Protocol) server for integration with the IT Admin Coordinator. This enables centralized monitoring and coordination of the clinic's AI scribe system.

**Agent ID:** `clinic-scribe`
**MCP Port:** `7101`
**Protocol:** JSON-RPC 2.0 over HTTP POST
**Transport:** HTTP (not SSE for V1)

---

## Quick Start

```bash
# Health check (simple GET endpoint)
curl http://localhost:7101/health

# List available tools
curl -X POST http://localhost:7101/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'

# Call a tool
curl -X POST http://localhost:7101/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"health_check","arguments":{}}}'
```

---

## Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/health` | GET | Quick health check, returns JSON (non-MCP) |
| `/mcp` | POST | JSON-RPC 2.0 MCP endpoint |

### `/health` Response

```json
{
  "healthy": true,
  "agent_id": "clinic-scribe",
  "timestamp": "2026-01-16T15:07:11.650610+00:00"
}
```

---

## MCP Protocol

All MCP requests use JSON-RPC 2.0 format:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "tool_name",
    "arguments": {}
  }
}
```

### Supported Methods

| Method | Description |
|--------|-------------|
| `tools/list` | List all available tools with schemas |
| `tools/call` | Execute a tool |

### Response Format

Success:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "{\"healthy\": true, ...}"
      }
    ]
  }
}
```

Error:
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Error message"
      }
    ],
    "isError": true
  }
}
```

---

## Available Tools (V1)

### 1. `agent_identity`

Returns the agent's identity and basic information.

**Arguments:** None

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "agent_identity",
    "arguments": {}
  }
}
```

**Response:**
```json
{
  "agent_id": "clinic-scribe",
  "agent_name": "AI Scribe Agent",
  "version": "0.1.0",
  "machine": "Arashs-Mac-mini.local",
  "mcp_port": 7101,
  "uptime_seconds": 3600,
  "started_at": "2026-01-16T15:07:05.149303+00:00",
  "capabilities": [
    "realtime_transcription_display",
    "soap_generation",
    "emr_integration",
    "template_management"
  ]
}
```

---

### 2. `health_check`

Quick health check for monitoring systems.

**Arguments:** None

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "health_check",
    "arguments": {}
  }
}
```

**Response:**
```json
{
  "healthy": true,
  "timestamp": "2026-01-16T15:07:30.868715+00:00",
  "checks": {
    "services_running": true,
    "disk_space_ok": true,
    "memory_ok": true,
    "dependencies_ok": true
  }
}
```

**Health Check Details:**
- `services_running`: Can access internal session state
- `disk_space_ok`: Models directory is writable
- `memory_ok`: Currently always true (placeholder for future)
- `dependencies_ok`: Required services (STT Router, LLM Router) are configured

---

### 3. `get_status`

Returns detailed operational status of the agent and its managed services.

**Arguments:** None

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "get_status",
    "arguments": {}
  }
}
```

**Response:**
```json
{
  "status": "healthy",
  "services": [
    {
      "name": "scribe-ui",
      "status": "running",
      "details": null
    },
    {
      "name": "whisper-remote",
      "status": "configured",
      "details": "http://10.241.15.154:8001"
    },
    {
      "name": "llm-router",
      "status": "configured",
      "details": "http://10.241.15.154:8080"
    },
    {
      "name": "medplum",
      "status": "configured",
      "details": "http://10.241.15.154:8103"
    }
  ],
  "last_activity": "2026-01-16T15:07:37.045348+00:00",
  "active_tasks": 0,
  "queued_tasks": 0,
  "error_count_last_hour": 0,
  "warnings": []
}
```

**Status Values:**
- `healthy`: All systems operational
- `degraded`: Operating with reduced capability (check `warnings`)
- `error`: Experiencing errors (check service details)
- `offline`: Not operational

**Service Status Values:**
- `running`: Service is active
- `configured`: External service URL is configured
- `not_configured`: External service URL is missing
- `error`: Service has an error (check `details`)
- `idle`, `recording`, `preparing`, `stopping`, `completed`: Session states for scribe-ui

**Active Tasks:**
- `0`: Idle, no recording in progress
- `1`: Recording session active

---

### 4. `get_logs`

Retrieves recent log entries from the agent's activity logs.

**Arguments:**

| Argument | Type | Default | Description |
|----------|------|---------|-------------|
| `lines` | integer | 50 | Number of lines (max: 500) |
| `level` | string | null | Filter: "all", "error", "warn", "info", "debug" |
| `service` | string | null | Filter by service name (partial match) |
| `since` | string | null | ISO 8601 timestamp to filter from |
| `search` | string | null | Text search in message or target |

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "get_logs",
    "arguments": {
      "lines": 10,
      "level": "error",
      "since": "2026-01-16T00:00:00Z"
    }
  }
}
```

**Response:**
```json
{
  "entries": [
    {
      "timestamp": "2026-01-16T14:45:19.485676Z",
      "level": "INFO",
      "service": "transcription_app_lib",
      "message": "Transcription App starting..."
    }
  ],
  "total_matching": 150,
  "truncated": true
}
```

**Log Levels (in priority order):**
1. `error` - Errors only
2. `warn` - Warnings and errors
3. `info` - Info, warnings, and errors
4. `debug` - All including debug
5. `all` - Everything including trace

**Common Services:**
- `transcription_app_lib` - Main app
- `activity_log` - Activity logging system
- `session` - Recording session management
- `pipeline` - Audio processing pipeline
- `mcp` - MCP server
- `medplum` - EMR integration

---

## Architecture Context

```
┌─────────────────────────────────────────────────────┐
│                   Tauri Process                      │
│                                                      │
│  ┌──────────────┐         ┌───────────────────────┐ │
│  │  React UI    │◄──IPC──►│     Rust Backend      │ │
│  │  (WebView)   │         │                       │ │
│  └──────────────┘         │  ┌─────────────────┐  │ │
│                           │  │ SessionManager  │  │ │
│                           │  │ (Arc<Mutex>)    │  │ │
│                           │  └────────┬────────┘  │ │
│                           │           │ shared    │ │
│                           │  ┌────────▼────────┐  │ │
│                           │  │ MCP Server      │  │ │
│                           │  │ (Axum :7101)    │  │ │
│                           │  └─────────────────┘  │ │
│                           └───────────────────────┘ │
└─────────────────────────────────────────────────────┘
                                    ▲
                                    │ HTTP/JSON-RPC
                                    ▼
                         ┌─────────────────────┐
                         │ IT Admin Coordinator│
                         └─────────────────────┘
```

The MCP server runs as an async task within the same Tauri process. It shares state directly with the main app via `Arc<Mutex<SessionManager>>`, providing real-time access to:
- Current session state (idle, recording, completed, error)
- Active task count
- Error messages

---

## Dependencies

This agent depends on external services:

| Service | Required | Purpose | Health Check |
|---------|----------|---------|--------------|
| STT Router | Yes | WebSocket streaming transcription | URL configured |
| LLM Router | Yes | SOAP note generation | URL configured |
| Medplum | No | EMR integration | URL configured |

**Dependency URLs are read from:** `~/.transcriptionapp/config.json`

---

## Configuration

The agent reads configuration from `~/.transcriptionapp/config.json`. Key fields relevant to MCP:

```json
{
  "whisper_server_url": "http://10.241.15.154:8001",
  "stt_alias": "medical-streaming",
  "stt_postprocess": true,
  "llm_router_url": "http://10.241.15.154:8080",
  "llm_api_key": "...",
  "llm_client_id": "ai-scribe",
  "medplum_server_url": "http://10.241.15.154:8103",
  "medplum_client_id": "..."
}
```

---

## Log Files

Logs are stored in: `~/.transcriptionapp/logs/`

- `activity.log` - Current day's log (JSON format, one entry per line)
- `activity.log.YYYY-MM-DD` - Rotated daily logs

**Log Entry Format (JSON):**
```json
{
  "timestamp": "2026-01-16T14:45:19.485676Z",
  "level": "INFO",
  "target": "transcription_app_lib::session",
  "fields": {
    "message": "Session transitioning to Recording"
  },
  "threadId": 1,
  "file": "src/session.rs",
  "line": 160
}
```

**Note:** Logs are PHI-safe. They contain session IDs, timestamps, and event types but never transcript content or patient information.

---

## Session States

The scribe-ui service reports these session states:

```
Idle → Preparing → Recording → Stopping → Completed
  ↑                                           │
  └──────────── Reset ←───────────────────────┘
        ↑
        └── Error (from any state)
```

| State | Description | `active_tasks` |
|-------|-------------|----------------|
| `idle` | Ready to start | 0 |
| `preparing` | Loading models | 0 |
| `recording` | Actively transcribing | 1 |
| `stopping` | Finishing transcription | 0 |
| `completed` | Session finished, reviewing | 0 |
| `error` | Error occurred | 0 |

---

## Error Handling

When a tool encounters an error, the response includes `isError: true`:

```json
{
  "content": [
    {
      "type": "text",
      "text": "Could not determine home directory"
    }
  ],
  "isError": true
}
```

Common errors:
- Config file not found or invalid
- Log directory not accessible
- Session state lock timeout (rare)

---

## CORS

The MCP server has CORS enabled with permissive settings:
- `Access-Control-Allow-Origin: *`
- `Access-Control-Allow-Methods: *`
- `Access-Control-Allow-Headers: *`

This allows the IT Admin Coordinator to connect from any origin.

---

## Future Tools (V2+)

These tools are planned but not yet implemented:

### `get_active_encounters`
Returns currently active or recent encounters.

### `get_encounter_details`
Returns details of a specific encounter including transcription stats.

### `get_templates`
Returns available SOAP note templates.

### `get_config` / `update_config`
Read and modify agent configuration.

---

## Network Configuration

For the IT Admin Coordinator to connect:

1. Ensure port 7101 is accessible on the machine running the scribe app
2. The scribe app binds to `0.0.0.0:7101` (all interfaces)
3. No authentication required for V1 (internal network assumed)

**Firewall rules (if needed):**
```bash
# macOS
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /path/to/transcription-app
sudo /usr/libexec/ApplicationFirewall/socketfilterfw --unblockapp /path/to/transcription-app
```

---

## Monitoring Recommendations

For the IT Admin Coordinator:

1. **Heartbeat:** Poll `/health` every 30 seconds
2. **Status check:** Call `get_status` every 60 seconds for detailed state
3. **Log aggregation:** Call `get_logs` with `since` parameter to collect new entries
4. **Alert thresholds:**
   - `healthy: false` → Alert
   - `status: degraded` → Warning
   - `status: error` → Alert
   - `active_tasks > 0` for extended period without completion → Warning

---

## Changelog

### V1 (2026-01-16)
- Initial MCP server implementation
- Tools: `agent_identity`, `health_check`, `get_status`, `get_logs`
- Monitor-only (no control tools)
