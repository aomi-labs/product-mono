# Deployment Monitoring Guide

## Testing Scripts

### 1. Backend Test (Raw IP)
**Script:** `scripts/test-backend-curl.sh`
- Tests direct backend connections without proxy
- Verifies Backend (8081), MCP (5001), and Anvil (8545) ports
- Usage: `./scripts/test-backend-curl.sh [host]`

### 2. Proxy Test (HTTPS)
**Script:** `scripts/test-proxy-curl.sh`  
- Tests nginx proxy with SSL/CORS
- Verifies all endpoints through proxy
- Usage: `./scripts/test-proxy-curl.sh <domain> [override-ip]`

## Deployment Workflow

1. **Deploy Backend**
   - Launch backend on cloud instance with public IP with `scripts/compose-backend-prod.sh`
   - Run `./scripts/test-backend-curl.sh <IP>` 
   - Ensure all backend services respond

2. **Launch Proxy**
   - Configure `.env` with correct domains
   - Run `./setup.sh [api-domain] [email]` in `docker/nginx/` with `AOMI_DOMAIN` and `AOMI_API_DOMAIN`
      - `AOMI_DOMAIN` is the frontend domain, backend doesn't need that. Nginx needs to know it for CORS.
   - Test with `./scripts/test-proxy-curl.sh [api-domain]`

3. **Connect Frontend**
   - Point frontend to `AOMI_API_DOMAIN` from `.env`

## Troubleshooting

When requests fail, test separately:
1. **Backend down?** → Run `test-backend-curl.sh` with raw IP
2. **Proxy issues?** → Run `test-proxy-curl.sh` with domain

This isolates whether the problem is with backend services or nginx proxy configuration.