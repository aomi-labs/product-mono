# Infrastructure & Deployment

This document describes Aomi's deployment architecture, database management, and operational procedures.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         GitHub Actions                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────────────────┐ │
│  │ ci.yml      │    │ docker-     │    │ deploy-v2.yml           │ │
│  │ (tests)     │    │ build.yml   │    │ (stateless deploy)      │ │
│  └──────┬──────┘    └──────┬──────┘    └───────────┬─────────────┘ │
│         │                  │                       │               │
│         │                  ▼                       │               │
│         │           ┌─────────────┐                │               │
│         │           │ ghcr.io     │◄───────────────┘               │
│         │           │ (images)    │                                │
│         │           └─────────────┘                                │
│         │                                                          │
│  ┌──────┴──────┐    ┌─────────────┐    ┌─────────────────────────┐ │
│  │ db-migrate  │    │ data-sync   │    │ (future)                │ │
│  │ .yml        │    │ .yml        │    │ backup.yml              │ │
│  └─────────────┘    └─────────────┘    └─────────────────────────┘ │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        Servers                                      │
├──────────────────────────────┬──────────────────────────────────────┤
│         Staging              │           Production                 │
│   (api.staging.aomi.dev)     │        (api.aomi.dev)                │
├──────────────────────────────┼──────────────────────────────────────┤
│  /opt/aomi/                  │  /opt/aomi/                          │
│  ├── docker-compose.yml      │  ├── docker-compose.yml              │
│  ├── providers.toml          │  ├── providers.toml                  │
│  └── (no git repo needed)    │  └── (no git repo needed)            │
└──────────────────────────────┴──────────────────────────────────────┘
```

## Workflows

### 1. Application Deployment (`deploy-v2.yml`)

**Trigger:** After successful Docker image build on `main` or `prod-v3`

**Flow:**
1. Checkout compose file from repo (sparse)
2. SSH to server
3. Pull Docker image from ghcr.io
4. Run `docker compose up -d`
5. Health check

**No git clone required on server.** Secrets passed via GitHub Secrets → environment variables.

### 2. Database Migrations (`db-migrate.yml`)

**Trigger:** Push to `main`/`prod-v3` with changes in `aomi/bin/backend/migrations/`

**Flow:**
1. Fetch migration files
2. Run `sqlx migrate run`
3. Verify migration status

**Runs independently from app deployment.**

### 3. Data Sync (`data-sync.yml`)

**Trigger:** 
- Scheduled: Every 6 hours (staging only)
- Manual: workflow_dispatch

**Flow:**
1. Fetch sync scripts from repo
2. Run `fetch_contracts.sh`
3. Report status

**Runs independently from app deployment.**

## Deployment

### Staging

Automatically deploys on push to `main`:

```
Push to main → CI passes → Docker build → Deploy to staging
```

### Production

Automatically deploys on push to `prod-v3` (requires approval):

```
Push to prod-v3 → CI passes → Docker build → [Approval] → Deploy to prod
```

### Manual Rollback

Images are tagged with commit SHA for easy rollback:

```bash
# SSH to server
cd /opt/aomi

# Check available tags
docker images ghcr.io/aomi-labs/product-mono/backend

# Rollback to specific version
export IMAGE_TAG=main-abc123f
docker compose down
docker compose up -d
```

## Database

### Connection Strings

| Environment | Secret Name | Format |
|-------------|-------------|--------|
| Staging | `STAGING_DATABASE_URL` | `postgres://user:pass@host:5432/chatbot` |
| Production | `PROD_DATABASE_URL` | `postgres://user:pass@host:5432/chatbot` |

### Tables

| Table | Purpose | Updated By |
|-------|---------|------------|
| `contracts` | Smart contract source + ABI | data-sync workflow |
| `transactions` | On-chain tx history | Backend (on-demand) |
| `sessions` | User chat sessions | Backend |
| `messages` | Chat history | Backend |
| `api_keys` | User API keys | Backend |

### Migrations

Located in `aomi/bin/backend/migrations/`. Naming convention:

```
YYYYMMDDHHMMSS_description.sql
```

Example:
```sql
-- 20250120000000_add_new_column.sql
ALTER TABLE users ADD COLUMN new_field TEXT;
```

**Rules:**
- Always backwards compatible
- No destructive changes without separate migration
- Test on staging first

## Server Setup (One-Time)

### Prerequisites

```bash
# Install Docker
curl -fsSL https://get.docker.com | sh

# Create deploy directory
mkdir -p /opt/aomi
cd /opt/aomi

# Copy providers.toml (from secure source)
# This file contains RPC endpoints and is not in the repo
```

### Required Files on Server

```
/opt/aomi/
├── docker-compose.yml    # Copied by deploy workflow
└── providers.toml        # Manual setup (contains RPC URLs)
```

## Secrets (GitHub)

| Secret | Used By | Description |
|--------|---------|-------------|
| `STAGING_SSH_HOST` | deploy, migrate | Staging server IP |
| `STAGING_SSH_USER` | deploy, migrate | SSH username |
| `STAGING_SSH_KEY` | deploy, migrate | SSH private key |
| `STAGING_DATABASE_URL` | migrate, data-sync | Postgres connection |
| `PROD_SSH_HOST` | deploy, migrate | Production server IP |
| `PROD_SSH_USER` | deploy, migrate | SSH username |
| `PROD_SSH_KEY` | deploy, migrate | SSH private key |
| `PROD_DATABASE_URL` | migrate, data-sync | Postgres connection |
| `ANTHROPIC_API_KEY` | deploy | AI API key |
| `OPENAI_API_KEY` | deploy | AI API key |
| `ETHERSCAN_API_KEY` | data-sync | Contract fetching |

## Monitoring

### Health Endpoints

| Environment | URL |
|-------------|-----|
| Staging | https://api.staging.aomi.dev/health |
| Production | https://api.aomi.dev/health |

### Logs

```bash
# SSH to server
cd /opt/aomi
docker compose logs -f backend
docker compose logs -f postgres
```

### Database Status

```bash
# Check connection count
psql $DATABASE_URL -c "SELECT count(*) FROM pg_stat_activity;"

# Check table sizes
psql $DATABASE_URL -c "SELECT relname, pg_size_pretty(pg_total_relation_size(relid)) FROM pg_stat_user_tables ORDER BY pg_total_relation_size(relid) DESC;"

# Check contracts count
psql $DATABASE_URL -c "SELECT chain, COUNT(*) FROM contracts GROUP BY chain;"
```
