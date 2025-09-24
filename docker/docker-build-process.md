# Docker Build Process Documentation

## 1. Multi-Stage Build Process

The Docker build process uses a multi-stage approach with three main build stages:


```mermaid
sequenceDiagram
    participant Source as Source Code
    participant RustBuilder as Rust Builder<br/>rustlang/rust:nightly-slim
    participant FrontendBuilder as Frontend Builder<br/>node:20-bullseye-slim
    participant BackendRuntime as Backend Runtime<br/>debian:bookworm-slim
    participant MCPRuntime as MCP Runtime<br/>debian:bookworm-slim
    participant FrontendRuntime as Frontend Runtime<br/>node:20-bullseye-slim

    Note over Source, FrontendRuntime: Multi-Stage Docker Build Process

    Source->>RustBuilder: COPY chatbot ./chatbot
    RustBuilder->>RustBuilder: Install pkg-config, libssl-dev, clang, make
    RustBuilder->>RustBuilder: cargo build --locked --release<br/>-p backend -p aomi-mcp

    Source->>FrontendBuilder: COPY frontend/package*.json
    FrontendBuilder->>FrontendBuilder: Install python3, make, g++
    FrontendBuilder->>FrontendBuilder: npm ci
    Source->>FrontendBuilder: COPY frontend/
    FrontendBuilder->>FrontendBuilder: npm run build

    RustBuilder-->>BackendRuntime: COPY backend binary<br/>(/workspace/chatbot/target/release/backend)
    BackendRuntime->>BackendRuntime: Install ca-certificates, curl
    Source->>BackendRuntime: COPY config.yaml, documents/
    Source->>BackendRuntime: COPY docker/backend-entrypoint.sh

    RustBuilder-->>MCPRuntime: COPY aomi-mcp-server binary<br/>(/workspace/chatbot/target/release/aomi-mcp-server)
    MCPRuntime->>MCPRuntime: Install ca-certificates, curl
    Source->>MCPRuntime: COPY docker/mcp-entrypoint.sh

    FrontendBuilder-->>FrontendRuntime: COPY .next/standalone
    FrontendBuilder-->>FrontendRuntime: COPY .next/static
    FrontendBuilder-->>FrontendRuntime: COPY public/
    Source->>FrontendRuntime: COPY package.json

    Note over BackendRuntime: Ready: Port 8081
    Note over MCPRuntime: Ready: Port 5001
    Note over FrontendRuntime: Ready: Port 3000
```


### Stage 1: Rust Builder
- **Base Image**: `rustlang/rust:nightly-slim` (required for Rust edition 2024)
- **Dependencies**: pkg-config, libssl-dev, clang, make
- **Output**: Compiles `backend` and `aomi-mcp-server` binaries
- **Build Command**: `cargo build --locked --release -p backend -p aomi-mcp`

### Stage 2: Frontend Builder
- **Base Image**: `node:20-bullseye-slim`
- **Dependencies**: python3, python-is-python3, make, g++
- **Process**:
  1. Install dependencies with `npm ci`
  2. Build Next.js application with `npm run build`
  3. Generates standalone output for production

### Stage 3: Runtime Images
Creates three separate minimal runtime containers:

#### Backend Runtime (`backend-runtime`)
- **Base**: `debian:bookworm-slim`
- **Port**: 8081
- **Binary**: `/usr/local/bin/backend`
- **Config**: Copies `config.yaml` and `documents/` directory
- **Entrypoint**: `/entrypoint.sh` with optional `--no-docs` flag

#### MCP Runtime (`mcp-runtime`)
- **Base**: `debian:bookworm-slim`
- **Port**: 5001
- **Binary**: `/usr/local/bin/aomi-mcp-server`
- **Config**: Supports `MCP_NETWORK_URLS_JSON` environment variable
- **Entrypoint**: `/entrypoint.sh` with flexible argument handling

#### Frontend Runtime (`frontend-runtime`)
- **Base**: `node:20-bullseye-slim`
- **Port**: 3000
- **Files**: Copies `.next/standalone`, `.next/static`, `public/`, `package.json`
- **Command**: `node server.js`

## 2. Docker Compose Service Architecture

```mermaid
graph LR
    subgraph "Production Environment"
        direction TB
        P_MCP[MCP Server<br/>Port: 5001<br/>Info Logging<br/>forge-mcp-mcp:latest]
        P_Backend[Backend API<br/>Port: 8081<br/>Docs Enabled<br/>forge-mcp-backend:latest]
        P_Frontend[Frontend<br/>Port: 3001<br/>Next.js<br/>forge-mcp-frontend:latest]
        P_Anvil[Anvil<br/>Port: 8545<br/>Optional Profile<br/>foundry:latest]

        P_MCP --> P_Backend
        P_Backend --> P_Frontend
    end

    subgraph "Development Environment"
        direction TB
        D_MCP[MCP Server<br/>Port: 5000<br/>Debug Logging<br/>Build Target: mcp-runtime]
        D_Backend[Backend API<br/>Port: 8080<br/>Debug + No Docs<br/>Build Target: backend-runtime]
        D_Frontend[Frontend<br/>Port: 3000<br/>Next.js<br/>Build Target: frontend-runtime]
        D_Anvil[Anvil<br/>Port: 8545<br/>Always On<br/>foundry:latest]

        D_MCP --> D_Backend
        D_Backend --> D_Frontend
    end

    subgraph "External"
        User[User Browser]
        ETH[Ethereum Mainnet<br/>via Alchemy RPC]
        EnvProd[.env.prod]
        EnvDev[.env.dev]
    end

    User --> P_Frontend
    User --> D_Frontend
    P_Anvil -.-> ETH
    D_Anvil -.-> ETH
    EnvProd --> P_MCP
    EnvProd --> P_Backend
    EnvProd --> P_Frontend
    EnvDev --> D_MCP
    EnvDev --> D_Backend
    EnvDev --> D_Frontend
```

### Production Configuration (`docker-compose.yml`)
```yaml
services:
  mcp:      # Port 5001, depends on: none
  backend:  # Port 8081, depends on: mcp
  frontend: # Port 3001, depends on: backend
  anvil:    # Port 8545, optional (profiles: dev, anvil)
```

### Development Configuration (`docker-compose.dev.yml`)
```yaml
services:
  mcp:      # Port 5000, debug logging
  backend:  # Port 8080, debug logging, docs disabled
  frontend: # Port 3000
  anvil:    # Port 8545, always enabled
```


## 3. Environment Configuration

### Production Environment (`.env.prod`)
- **Backend**: Port 8081, info logging, docs enabled
- **MCP**: Port 5001, info logging
- **Frontend**: Port 3001, backend URL points to port 8081

### Development Environment (`.env.dev`)
- **Backend**: Port 8080, debug logging, docs disabled
- **MCP**: Port 5000, debug logging
- **Frontend**: Port 3000, backend URL points to port 8080

### Key Environment Variables
- `BACKEND_SKIP_DOCS`: Controls documentation generation
- `MCP_NETWORK_URLS_JSON`: JSON configuration for MCP server
- `ETH_RPC_URL`: Ethereum RPC endpoint for Anvil fork
- `RUST_LOG`: Logging level (info/debug)

## 4. Build Optimization Features

### .dockerignore
Excludes development files and build artifacts:
- Git repository (`.git/`)
- Node modules (`frontend/node_modules/`)
- Rust build artifacts (`chatbot/target/`)
- Development tools (`.vscode/`, `.idea/`)
- Large documentation bundles (`docs/`)

### Dependency Caching
- **Rust**: Uses `--locked` flag for reproducible builds
- **Node.js**: Copies `package*.json` first for layer caching
- **System packages**: Removes package lists after installation

## 5. Service Dependencies and Startup Order

```mermaid
graph TB
    subgraph "Startup Sequence"
        Start([Container Start]) --> MCP[1. MCP Server<br/>Port: 5001/5000<br/>No Dependencies]
        MCP --> Backend[2. Backend API<br/>Port: 8081/8080<br/>Depends on: MCP]
        Backend --> Frontend[3. Frontend<br/>Port: 3001/3000<br/>Depends on: Backend]

        Start --> Anvil[Anvil Blockchain<br/>Port: 8545<br/>Independent]
    end

    subgraph "Health Checks"
        MCP --> MCP_Ready{MCP Ready?}
        Backend --> Backend_Ready{Backend Ready?}
        MCP_Ready -->|Yes| Backend
        Backend_Ready -->|Yes| Frontend
    end
```

1. **MCP Server** starts first (no dependencies)
2. **Backend** waits for MCP server to be ready
3. **Frontend** waits for backend to be ready
4. **Anvil** runs independently for blockchain simulation

## 6. Image Naming Convention
- `forge-mcp-backend:latest`
- `forge-mcp-mcp:latest`
- `forge-mcp-frontend:latest`

## 7. Entrypoint Scripts

### Backend Entrypoint (`docker/backend-entrypoint.sh`)
- Conditionally adds `--no-docs` flag based on `BACKEND_SKIP_DOCS`
- Supports additional command line arguments

### MCP Entrypoint (`docker/mcp-entrypoint.sh`)
- Supports command line arguments
- Falls back to `MCP_NETWORK_URLS_JSON` environment variable
- Defaults to running server without arguments
