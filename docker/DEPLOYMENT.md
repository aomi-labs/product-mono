# Production Deployment Guide

Comprehensive guide for deploying the Forge MCP Backend platform to production environments.

## Table of Contents
- [Architecture Overview](#architecture-overview)
- [Deployment Options](#deployment-options)
- [Prerequisites](#prerequisites)
- [Quick Start](#quick-start)
- [Detailed Deployment Steps](#detailed-deployment-steps)
- [Environment Configuration](#environment-configuration)
- [Security Hardening](#security-hardening)


## Architecture Overview

The platform consists of four core services that can be deployed together or separately:

```mermaid
graph TB
    subgraph "External Services"
        Claude[Anthropic Claude API]
        Brave[Brave Search API]
        Etherscan[Etherscan API]
        Alchemy[Alchemy RPC]
    end
    
    subgraph "Production Stack"
        Frontend[Frontend<br/>Next.js:3000]
        Backend[Backend API<br/>Rust:8081]
        MCP[MCP Server<br/>Rust:5001]
        Anvil[Anvil Fork<br/>Ethereum:8545]
        
        Frontend --> Backend
        Backend --> MCP
        MCP --> Anvil
    end
    
    MCP --> Claude
    MCP --> Brave
    MCP --> Etherscan
    Anvil --> Alchemy
    
    style Frontend fill:#9cf
    style Backend fill:#fc9
    style MCP fill:#fc9
    style Anvil fill:#cfc
```

## Deployment Options

### Option 1: Monolithic Docker Deployment (Recommended for Single Server)
- All services in one docker-compose stack
- Suitable for single VPS/dedicated server
- Easiest to manage and monitor

### Option 2: Distributed Deployment (For Scale)
- Backend services on one server
- Frontend on CDN/Vercel
- Database/cache on managed services
- Better for high traffic scenarios

### Option 3: Kubernetes Deployment (Enterprise)
- Full orchestration with auto-scaling
- Service mesh for internal communication
- Advanced monitoring and observability

### Option 4: Managed Cloud Services
- AWS ECS/Fargate
- Google Cloud Run
- Azure Container Instances
- DigitalOcean App Platform

## Prerequisites

### System Requirements
- **OS**: Ubuntu 22.04 LTS or compatible Linux distribution
- **CPU**: 4+ cores recommended
- **RAM**: 8GB minimum, 16GB recommended
- **Storage**: 50GB+ SSD
- **Network**: Static IP, ports 80/443 open

### Software Requirements
```bash
# Install Docker
curl -fsSL https://get.docker.com -o get-docker.sh
sudo sh get-docker.sh

# Install Docker Compose
sudo curl -L "https://github.com/docker/compose/releases/latest/download/docker-compose-$(uname -s)-$(uname -m)" -o /usr/local/bin/docker-compose
sudo chmod +x /usr/local/bin/docker-compose

# Install Python 3 (for configuration scripts)
sudo apt update && sudo apt install -y python3 python3-pip python3-yaml

# Install Git
sudo apt install -y git
```

### API Keys Required
1. **Anthropic Claude** (Required): https://console.anthropic.com/
2. **Brave Search** (Recommended): https://api.search.brave.com/
3. **Etherscan** (Recommended): https://etherscan.io/apis
4. **Alchemy** (Recommended): https://www.alchemy.com/

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/your-org/product-mono.git
cd product-mono

# 2. Set up environment
cp .env.template .env.prod
nano .env.prod  # Add your API keys

# 3. Deploy using pre-built images
./scripts/compose-backend-prod.sh latest

# 4. Verify deployment
curl http://your-server-ip:8081/health
```

## Detailed Deployment Steps

### Step 1: Server Preparation

```bash
# Update system
sudo apt update && sudo apt upgrade -y

# Set up firewall
sudo ufw allow 22/tcp    # SSH
sudo ufw allow 80/tcp    # HTTP
sudo ufw allow 443/tcp   # HTTPS
sudo ufw allow 3000/tcp  # Frontend
sudo ufw allow 8081/tcp  # Backend API
sudo ufw allow 5001/tcp  # MCP Server
sudo ufw allow 8545/tcp  # Anvil RPC
sudo ufw enable

# Create deployment user
sudo useradd -m -s /bin/bash forge
sudo usermod -aG docker forge
sudo su - forge
```

### Step 2: Repository Setup

```bash
# Clone repository
git clone https://github.com/your-org/product-mono.git
cd product-mono

# Create environment file
cp .env.template .env.prod
```

### Step 3: Configure Environment

Edit `.env.prod` with your API keys:

```bash
# Required
ANTHROPIC_API_KEY=sk-ant-api03-your-key-here

# Recommended for full functionality
BRAVE_SEARCH_API_KEY=your-brave-key
ETHERSCAN_API_KEY=your-etherscan-key
ALCHEMY_API_KEY=your-alchemy-key

# Optional
ZEROX_API_KEY=your-0x-key

# Network RPC URLs (if using custom endpoints)
ETH_RPC_URL=https://eth-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}
BASE_RPC_URL=https://base-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}
ARBITRUM_RPC_URL=https://arb-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}
```

### Step 4: Build or Pull Docker Images
#### Option A: One-shot script
One-shot script to pull backend images and run. Check basic enpoints after spinning up containers.
```bash
./scripts/compose-backend-prod.sh
```

#### Option B: Use Pre-built Images (Faster)
```bash
# Pull from GitHub Container Registry
export IMAGE_TAG=latest
docker pull ghcr.io/your-org/product-mono/backend:$IMAGE_TAG
docker pull ghcr.io/your-org/product-mono/mcp:$IMAGE_TAG
docker pull ghcr.io/your-org/product-mono/frontend:$IMAGE_TAG
```

#### Option C: Build Locally (Customizable)
```bash
# Build all images
./scripts/compose-build-monolithic.sh

# Or build individually
docker build --target backend-runtime -t forge-mcp/backend .
docker build --target mcp-runtime -t forge-mcp/mcp .
docker build --target frontend-runtime -t forge-mcp/frontend .
```

### Step 4.1: (No Script) Deploy Services


#### Backend Services Only (No Frontend)

```bash
docker compose -f docker/docker-compose-backend.yml up -d
```

#### Full Stack (All Services)
```bash
docker compose -f docker/docker-compose-monolithic.yml up -d
```

### Step 5: Set Up NGINX Reverse Proxy (Optional but Recommended)

```bash
cd docker/nginx
cp .env.template .env

# Edit .env with your domains
nano .env

# Run setup script for SSL certificates
./setup.sh api.yourdomain.com admin@yourdomain.com

# Start NGINX proxy
docker compose up -d
```

### Step 6: Verify Deployment

```bash
# Check container status
docker ps

# Test backend health
curl http://localhost:8081/health

# Test MCP server
curl http://localhost:5001/health

# Check logs
docker compose logs -f backend
docker compose logs -f mcp
```

## Environment Configuration

### Port Configuration

Default production ports can be customized via environment variables:

```bash
# In .env.prod or docker-compose override
BACKEND_PORT=8081        # Backend API port
MCP_SERVER_PORT=5001     # MCP server port
FRONTEND_PORT=3000       # Frontend port
ANVIL_PORT=8545         # Anvil RPC port
```


### CORS Configuration

For frontend on different domain:

```bash
# In .env.prod
BACKEND_ALLOWED_ORIGINS=https://app.yourdomain.com,https://yourdomain.com
# Or append to defaults
BACKEND_EXTRA_ALLOWED_ORIGINS=https://preview.yourdomain.com
```

## Security Hardening

### 1. Use Environment-Specific Secrets

```bash
# Generate strong random values
openssl rand -hex 32  # For session secrets

# Store in secure location
chmod 600 .env.prod
```

### 2. Enable HTTPS (Required for Production)

Use the NGINX proxy setup with Let's Encrypt:

```bash
cd docker/nginx
./setup.sh api.yourdomain.com admin@yourdomain.com
```

### 3. Implement Rate Limiting

Add to NGINX configuration:

```nginx
http {
    limit_req_zone $binary_remote_addr zone=api:10m rate=10r/s;
    
    server {
        location /api/ {
            limit_req zone=api burst=20 nodelay;
            # ... existing config
        }
    }
}
```

### 4. Docker Security

```bash
# Run containers as non-root
docker run --user 1000:1000 ...

# Use read-only filesystem where possible
docker run --read-only ...

# Limit resources
docker run --memory="2g" --cpus="2" ...
```