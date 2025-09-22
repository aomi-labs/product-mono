# Proposed Multi-User Deployment Architecture

## Overview
Complete architecture for supporting multiple concurrent users with session-based backend and stateless network-specific MCP servers.

## Current Single-User Architecture ❌
```
┌─────────────────────────────────────────────────────────────┐
│                     Single User Only                       │
└─────────────────────────────────────────────────────────────┘

Frontend (Port 3000)
    │
    ▼ HTTP/SSE
Backend (Port 8080) ◄── Single WebChatState + Single Agent
    │                   └── Shared Message History
    ▼ MCP/HTTP          └── Global Processing State
MCP Server (Port 5000) ◄── CombinedTool with current_network
    │                   └── Shared Network State (RwLock)
    ▼
Network Tools (testnet, mainnet, polygon...)

❌ Problems: User A and User B interfere with each other
```

## Proposed Multi-User Architecture ✅
```
┌─────────────────────────────────────────────────────────────┐
│                    Multi-User Support                      │
└─────────────────────────────────────────────────────────────┘

Frontend 1 (session_1) ─┐
Frontend 2 (session_2) ─┤
Frontend N (session_N) ─┘
    │ HTTP/SSE + session_id
    ▼
Backend (Port 8080) ◄── SessionManager
│                      ├── session_1: WebChatState + Agent
│                      ├── session_2: WebChatState + Agent
│                      └── session_N: WebChatState + Agent
├─ /api/mcp/testnet ────┐
├─ /api/mcp/mainnet ────┤
├─ /api/mcp/polygon ────┤ MCP Proxy Layer
├─ /api/mcp/base ───────┤
└─ /api/mcp/arbitrum ───┘
    │ HTTP Proxy
    ▼
MCP Testnet (Port 5000) ◄── NetworkSpecificTool(testnet)
MCP Mainnet (Port 5001) ◄── NetworkSpecificTool(mainnet)
MCP Polygon (Port 5002) ◄── NetworkSpecificTool(polygon)
MCP Base    (Port 5003) ◄── NetworkSpecificTool(base)
MCP Arbitrum(Port 5004) ◄── NetworkSpecificTool(arbitrum)

✅ Benefits: Complete user isolation + concurrent network usage
```

## Component Details

### 1. Frontend Layer
```typescript
// Each browser tab/window = unique session
class ChatManager {
  private sessionId: string = generateUniqueId(); // e.g., "user_12345_67890"
  private currentNetwork: string = 'testnet';

  // All API calls include session_id
  async sendMessage(message: string) {
    fetch('/api/chat', {
      method: 'POST',
      body: JSON.stringify({ message, session_id: this.sessionId })
    });
  }

  // Network selection is client-side only
  switchNetwork(network: string) {
    this.currentNetwork = network; // No server call needed!
  }

  // MCP calls route to network-specific endpoints
  async callMcp(tool: string, params: any) {
    fetch(`/api/mcp/${this.currentNetwork}`, {
      method: 'POST',
      body: JSON.stringify({ tool, params })
    });
  }
}
```

### 2. Backend Layer (Session Management)
```rust
// Session-isolated backend
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<Mutex<WebChatState>>>>>,
    last_activity: Arc<RwLock<HashMap<String, Instant>>>,
    cleanup_task: JoinHandle<()>,
}

// Each session gets:
// - Independent WebChatState
// - Dedicated agent process
// - Isolated message history
// - Separate processing state
```

### 3. MCP Layer (Network-Specific Servers)
```rust
// Each network = separate MCP server process
MCP Testnet:  NetworkSpecificTool { network: "testnet",  rpc: "http://127.0.0.1:8545" }
MCP Mainnet:  NetworkSpecificTool { network: "mainnet",  rpc: "https://eth-mainnet.g.alchemy.com/..." }
MCP Polygon:  NetworkSpecificTool { network: "polygon",  rpc: "https://polygon-mainnet.g.alchemy.com/..." }

// No shared state between networks
// No set_network tool (eliminated entirely)
```

## Deployment Configuration

### Docker Compose
```yaml
version: '3.8'
services:
  # Anvil (local testnet)
  anvil:
    image: ghcr.io/foundry-rs/foundry:latest
    command: ["anvil", "--host", "0.0.0.0", "--fork-url", "${MAINNET_FORK_URL}"]
    ports: ["8545:8545"]

  # Multiple MCP Servers (one per network)
  mcp-testnet:
    build: .
    command: ["./bin/mcp-server", "testnet", "http://anvil:8545"]
    ports: ["5000:5000"]
    depends_on: [anvil]

  mcp-mainnet:
    build: .
    command: ["./bin/mcp-server", "mainnet", "${ETH_RPC_URL}"]
    ports: ["5001:5001"]
    environment:
      - ETHERSCAN_API_KEY=${ETHERSCAN_API_KEY}

  mcp-polygon:
    build: .
    command: ["./bin/mcp-server", "polygon", "${POLYGON_RPC_URL}"]
    ports: ["5002:5002"]
    environment:
      - ETHERSCAN_API_KEY=${ETHERSCAN_API_KEY}

  mcp-base:
    build: .
    command: ["./bin/mcp-server", "base", "${BASE_RPC_URL}"]
    ports: ["5003:5003"]

  # Session-aware backend
  backend:
    build: .
    command: ["./bin/backend"]
    ports: ["8080:8080"]
    depends_on: [mcp-testnet, mcp-mainnet, mcp-polygon, mcp-base]
    environment:
      - SESSION_TIMEOUT=1800  # 30 minutes
      - MAX_SESSIONS=100

  # Frontend
  frontend:
    build: .
    command: ["npm", "run", "dev", "--", "--host", "0.0.0.0"]
    ports: ["3000:3000"]
    depends_on: [backend]

  # Optional: Redis for session persistence (future)
  # redis:
  #   image: redis:alpine
  #   ports: ["6379:6379"]
```

### Process Startup Order
```bash
# 1. Start Anvil (testnet)
docker-compose up anvil

# 2. Start MCP servers (parallel)
docker-compose up -d mcp-testnet mcp-mainnet mcp-polygon mcp-base

# 3. Start session-aware backend
docker-compose up -d backend

# 4. Start frontend
docker-compose up -d frontend

# Health check all services
curl http://localhost:8080/health
curl http://localhost:5000/health  # testnet MCP
curl http://localhost:5001/health  # mainnet MCP
```

## Resource Requirements

### Development Environment
```yaml
Resources per component:
- Frontend:    ~50MB RAM, 0.1 CPU
- Backend:     ~100MB RAM + (50MB per active session), 0.3 CPU
- MCP Server:  ~80MB RAM per network, 0.2 CPU each
- Anvil:       ~200MB RAM, 0.1 CPU

Total for 5 networks + 10 concurrent users:
- RAM:  ~1.5GB (Frontend + Backend + 5x MCP + Anvil + 10x Sessions)
- CPU:  ~1.5 cores
- Ports: 3000 (FE), 8080 (BE), 5000-5004 (MCP), 8545 (Anvil)
```

### Production Environment
```yaml
Scaling considerations:
- Backend:     Scale horizontally with load balancer + session affinity
- MCP Servers: Scale per network independently
- Database:    Add Redis/PostgreSQL for session persistence
- Monitoring:  Add health checks and metrics

Production resources (100 concurrent users):
- RAM:  ~8GB (1GB base + 5GB sessions + 2GB buffers)
- CPU:  ~4 cores
- Network: 1Gbps (for RPC calls)
```

## Migration Strategy

### Phase 1: Backend Sessions (Low Risk)
1. ✅ Add session_id to request structs
2. Implement SessionManager (backward compatible)
3. Update API endpoints to use sessions
4. Test with single user (should work identically)

### Phase 2: MCP Stateless (Medium Risk)
1. Create NetworkSpecificTool (duplicate existing logic)
2. Update MCP main.rs for network parameter
3. Start multiple MCP servers
4. Add backend MCP proxy endpoints
5. Test each network independently

### Phase 3: Frontend Integration (Low Risk)
1. Add session ID generation
2. Update API calls to include session_id
3. Change network switching to endpoint routing
4. Test multi-user scenarios

### Phase 4: Production Deployment (Medium Risk)
1. Update Docker Compose
2. Configure environment variables
3. Add monitoring and health checks
4. Performance testing with concurrent users

## Testing Strategy

### Unit Tests
- SessionManager session isolation
- NetworkSpecificTool network isolation
- MCP proxy routing correctness

### Integration Tests
- Multiple users concurrent chat
- Cross-network operations (user A on mainnet, user B on polygon)
- Session cleanup after timeout
- MCP server failure handling

### Load Tests
- 50+ concurrent users
- Network switching under load
- Session memory usage over time
- MCP server response times

## Benefits Summary
- ✅ **True Multi-User**: Complete session isolation
- ✅ **Concurrent Networks**: Users can use different networks simultaneously
- ✅ **Horizontal Scaling**: Add more backend/MCP instances easily
- ✅ **Fault Tolerance**: Individual component failures don't affect others
- ✅ **Resource Efficient**: Only active sessions consume resources
- ✅ **Stateless MCP**: No shared state conflicts between networks
- ✅ **Clean Architecture**: Single responsibility per component

## Risk Mitigation
- **Memory Leaks**: Automatic session cleanup after timeout
- **Resource Exhaustion**: Configurable max sessions per backend
- **Network Failures**: Individual MCP server health monitoring
- **Data Loss**: Optional Redis persistence for session recovery
- **Security**: Session IDs are unpredictable and timeout automatically