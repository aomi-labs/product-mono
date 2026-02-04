# Aomi Docker Deployment

## Architecture

The deployment is split into two layers:

```
┌─────────────────────────────────────────────────────────────┐
│                    INFRASTRUCTURE LAYER                      │
│                   (Persistent, run once)                     │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  docker-compose-db.yml                              │    │
│  │  ┌─────────────┐                                    │    │
│  │  │  postgres   │ ← Data survives all deploys       │    │
│  │  │  (aomi-db)  │                                    │    │
│  │  └─────────────┘                                    │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ Docker Network (aomi-network)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                    APPLICATION LAYER                         │
│               (Deployed by CI on every push)                 │
│  ┌─────────────────────────────────────────────────────┐    │
│  │  docker-compose-backend.yml                         │    │
│  │  ┌─────────────┐                                    │    │
│  │  │  backend    │ ← New image on every deploy       │    │
│  │  │ (aomi-be)   │                                    │    │
│  │  └─────────────┘                                    │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Files

| File | Purpose |
|------|---------|
| `docker-compose-db.yml` | Database infrastructure (run once, persistent) |
| `docker-compose-backend.yml` | Backend application (deployed by CI) |
| `setup-db.sh` | One-time database initialization script |

## Initial Server Setup

Run these commands **once** when setting up a new server:

```bash
# 1. Clone the repo
cd /opt
git clone https://github.com/aomi-labs/product-mono.git aomi
cd aomi/docker

# 2. Initialize the database
chmod +x setup-db.sh
./setup-db.sh  # Or: ./setup-db.sh your_secure_password

# 3. The backend will be deployed automatically by CI
```

## How CI Deployments Work

1. CI builds new Docker image
2. CI connects to server via SSH
3. CI verifies database is healthy (does NOT restart it)
4. CI stops only the backend container
5. CI starts backend with new image
6. Database remains running throughout

## Manual Operations

### Check Status
```bash
# Database status
docker ps | grep aomi-db
docker exec aomi-db pg_isready -U aomi -d chatbot

# Backend status
docker ps | grep aomi-backend
curl http://localhost:8081/health
```

### Restart Backend (preserves DB)
```bash
cd /opt/aomi/docker
docker compose -f docker-compose-backend.yml down
docker compose -f docker-compose-backend.yml up -d
```

### Restart Database (preserves data)
```bash
cd /opt/aomi/docker
docker compose -f docker-compose-db.yml restart
```

### View Logs
```bash
docker logs aomi-db      # Database logs
docker logs aomi-backend # Backend logs
```

## Database Migrations

Migrations are handled by the backend application on startup, NOT by recreating the database container. The backend uses SQLx migrations that run automatically when the application starts.

## Environment Variables

### Database (`docker-compose-db.yml`)
| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_PASSWORD` | `aomi_local` | Database password |
| `POSTGRES_PORT` | `5432` | External port |

### Backend (`docker-compose-backend.yml`)
| Variable | Default | Description |
|----------|---------|-------------|
| `IMAGE_TAG` | `latest` | Docker image tag |
| `ANTHROPIC_API_KEY` | (required) | Anthropic API key |
| `OPENAI_API_KEY` | (optional) | OpenAI API key |
| `DATABASE_URL` | (auto) | Override database URL |
| `BACKEND_PORT` | `8081` | API port |
| `RUST_LOG` | `info` | Log level |
