# Stateless MCP Server Architecture

## Overview
Redesign MCP servers to be stateless and network-specific, eliminating the shared `current_network` state that prevents multi-user support. Each network runs its own dedicated MCP server instance.

## Current Problems ❌
- **Shared Network State**: `CombinedTool.current_network: Arc<RwLock<String>>` conflicts between users
- **Network Switching**: `set_network` command changes global state affecting all users
- **Single MCP Instance**: One server tries to handle all networks and users

## Proposed Solution ✅

### 1. Network-Specific MCP Servers
```
MCP Server Instances:
├── mcp-testnet (port 5000) -> testnet tools only
├── mcp-mainnet (port 5001) -> mainnet tools only
├── mcp-polygon (port 5002) -> polygon tools only
├── mcp-base (port 5003) -> base tools only
└── mcp-arbitrum (port 5004) -> arbitrum tools only
```

### 2. Stateless MCP Tool Structure
```rust
// Remove CombinedTool, replace with NetworkSpecificTool
pub struct NetworkSpecificTool {
    cast_tool: CastTool,                    // Single network only
    network_name: String,                   // Fixed at startup
    brave_search_tool: Option<BraveSearchTool>, // Shared across networks
    etherscan_tool: Option<EtherscanTool>,      // Network-aware
    zerox_tool: Option<ZeroXTool>,              // Network-aware
    // Removed: current_network, cast_tools HashMap, set_network tool
}

impl NetworkSpecificTool {
    // Create tool for specific network only
    pub async fn new(network_name: &str, rpc_url: &str) -> Result<Self> {
        let cast_tool = CastTool::new_with_network(network_name.to_string(), rpc_url).await?;
        Ok(Self {
            cast_tool,
            network_name: network_name.to_string(),
            // ... initialize other tools
        })
    }

    // Remove set_network entirely - no network switching
    // All cast operations use self.cast_tool directly
}
```

### 3. MCP Server Startup per Network
```rust
// chatbot/crates/mcp-server/src/main.rs
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Required: network name argument
    let network_name = args.get(1)
        .ok_or_else(|| eyre::eyre!("Usage: mcp-server <network_name> [rpc_url]"))?;

    let rpc_url = args.get(2)
        .map(|s| s.as_str())
        .unwrap_or_else(|| default_rpc_url(network_name));

    tracing::info!("Starting {} MCP server with RPC: {}", network_name, rpc_url);

    // Create network-specific tool (no set_network capability)
    let tool = NetworkSpecificTool::new(network_name, rpc_url).await?;

    // Bind to network-specific port
    let port = network_port(network_name);
    let bind_addr = format!("{}:{}", &*MCP_SERVER_HOST, port);

    // ... rest of server setup
}

fn network_port(network: &str) -> u16 {
    match network {
        "testnet" => 5000,
        "mainnet" => 5001,
        "polygon" => 5002,
        "base" => 5003,
        "arbitrum" => 5004,
        _ => 5099 // fallback
    }
}

fn default_rpc_url(network: &str) -> &'static str {
    match network {
        "testnet" => "http://127.0.0.1:8545", // Anvil
        _ => panic!("RPC URL required for network: {}", network)
    }
}
```

### 4. Backend Network Routing
```rust
// chatbot/bin/backend/src/main.rs - Add MCP proxy endpoints
async fn create_router(session_manager: Arc<SessionManager>) -> Router {
    Router::new()
        .route("/api/chat", post(chat_endpoint))
        .route("/api/chat/stream", get(chat_stream))
        // Network-specific MCP endpoints
        .route("/api/mcp/testnet", post(mcp_proxy_endpoint))
        .route("/api/mcp/mainnet", post(mcp_proxy_endpoint))
        .route("/api/mcp/polygon", post(mcp_proxy_endpoint))
        .route("/api/mcp/base", post(mcp_proxy_endpoint))
        .route("/api/mcp/arbitrum", post(mcp_proxy_endpoint))
        .with_state(session_manager)
}

async fn mcp_proxy_endpoint(
    uri: Uri,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Extract network from path: /api/mcp/mainnet -> "mainnet"
    let network = uri.path()
        .strip_prefix("/api/mcp/")
        .unwrap_or("testnet");

    let mcp_port = network_port(network);

    // Proxy to network-specific MCP server
    let client = reqwest::Client::new();
    let response = client
        .post(&format!("http://127.0.0.1:{}/", mcp_port))
        .json(&request)
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let result = response.json().await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}
```

## Implementation Steps

### Phase 1: MCP Server Redesign
1. Create `NetworkSpecificTool` struct (remove `CombinedTool`)
2. Update `main.rs` to accept network parameter
3. Remove `set_network` tool and `current_network` state
4. Add network-specific port binding

### Phase 2: Multi-Instance Startup
1. Update Docker/scripts to start multiple MCP servers
2. Configure each server with network name and RPC URL
3. Test each MCP server independently

### Phase 3: Backend Integration
1. Add MCP proxy endpoints in backend
2. Route requests to appropriate MCP server
3. Remove old `/api/mcp-command` endpoint

### Phase 4: Frontend Updates
1. Replace network switching with endpoint selection
2. Route MCP calls to `/api/mcp/{network}` based on selected network
3. Remove network switching UI (just selection)

## File Changes Required

### MCP Server (`chatbot/crates/mcp-server/src/`)
- `main.rs`: Accept network parameter, bind to network port (~20 lines)
- `combined_tool.rs`: Replace with `network_specific_tool.rs` (~100 lines)
- Remove `set_network` tool entirely

### Backend (`chatbot/bin/backend/src/main.rs`)
- Add MCP proxy endpoints (~30 lines)
- Add network routing logic (~15 lines)
- Remove old MCP command handling (~20 lines)

### Deployment (`docker-compose.yml`, scripts/)
- Add multiple MCP server services (~20 lines)
- Update startup scripts (~10 lines)

## Benefits
- ✅ **Zero Shared State**: Each MCP server is completely independent
- ✅ **True Multi-User**: Users can use different networks simultaneously
- ✅ **Simple Scaling**: Add new networks by starting new servers
- ✅ **Fault Isolation**: Network server crash doesn't affect others
- ✅ **Clear Responsibilities**: Each server handles one network only

## Example Usage
```bash
# Start multiple MCP servers
./mcp-server testnet "http://127.0.0.1:8545" &    # port 5000
./mcp-server mainnet "$ETH_RPC_URL" &             # port 5001
./mcp-server polygon "$POLYGON_RPC_URL" &         # port 5002

# Frontend calls
POST /api/mcp/testnet { "tool": "balance", "params": {...} }
POST /api/mcp/mainnet { "tool": "balance", "params": {...} }
POST /api/mcp/polygon { "tool": "balance", "params": {...} }
```

## Considerations
- **Resource Usage**: Each network = 1 MCP server process
- **Health Monitoring**: Need to monitor multiple MCP servers
- **Service Discovery**: Backend needs to know which networks are available
- **Error Handling**: Handle individual MCP server failures gracefully
- **Startup Order**: MCP servers should start before backend