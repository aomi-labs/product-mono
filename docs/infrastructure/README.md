# Infrastructure & Deployment

## Workflows

| Workflow | Purpose | Trigger |
|----------|---------|---------|
| `ci.yml` | Tests (Rust + Frontend) | PR, push to main |
| `build-and-deploy.yml` | Build images + deploy | Push to main/prod-v3 |

That's it. Two workflows.

## How It Works

```
Push to main
    │
    ▼
┌─────────────────────────┐
│  build-and-deploy.yml   │
│  ┌───────────────────┐  │
│  │ 1. Build images   │  │
│  │ 2. Push to ghcr   │  │
│  │ 3. SSH to server  │  │
│  │ 4. docker pull    │  │
│  │ 5. docker up      │  │
│  │ 6. Health check   │  │
│  └───────────────────┘  │
└─────────────────────────┘
    │
    ▼
  Staging live (api.staging.aomi.dev)
```

Same flow for `prod-v3` → Production (with approval gate).

## Branches

| Branch | Deploys To |
|--------|------------|
| `main` | Staging |
| `prod-v3` | Production (requires approval) |

## Rollback

Images tagged with SHA for easy rollback:

```bash
# On server
cd /opt/aomi
IMAGE_TAG=main-abc123f docker compose -f docker-compose-backend.yml up -d
```

## Server Setup (One-Time)

```bash
mkdir -p /opt/aomi
# Copy providers.toml (contains RPC endpoints)
```

Required files on server:
```
/opt/aomi/
├── docker-compose-backend.yml  # Copied by CI
└── providers.toml              # Manual (RPC config)
```

## Database

**Migrations:** Run automatically on backend startup via sqlx.

**Contract sync:** Run manually or via cron on server:
```bash
# Add to crontab if needed
0 */6 * * * /opt/aomi/scripts/sync-contracts.sh
```

## Secrets (GitHub)

| Secret | Description |
|--------|-------------|
| `STAGING_SSH_HOST` | Staging server IP |
| `STAGING_SSH_USER` | SSH username |
| `STAGING_SSH_KEY` | SSH private key |
| `PROD_SSH_HOST` | Production server IP |
| `PROD_SSH_USER` | SSH username |
| `PROD_SSH_KEY` | SSH private key |
| `ANTHROPIC_API_KEY` | AI API key |
| `OPENAI_API_KEY` | AI API key |

## Monitoring

```bash
# Logs
docker compose -f docker-compose-backend.yml logs -f

# Health
curl https://api.staging.aomi.dev/health
curl https://api.aomi.dev/health
```
