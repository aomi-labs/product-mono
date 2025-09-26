#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}" )" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Parse arguments
if [[ $# -lt 2 ]]; then
    echo "❌ Error: IMAGE_TAG and AOMI_API_DOMAIN are required"
    echo "Usage: $0 <IMAGE_TAG> <AOMI_API_DOMAIN> [EMAIL]"
    echo "Example: $0 deployment-ver2 api.aomi.dev admin@aomi.dev"
    echo "Example: $0 latest api.example.com"
    exit 1
fi

IMAGE_TAG="$1"
AOMI_API_DOMAIN="$2"
EMAIL="${3:-admin@$AOMI_API_DOMAIN}"

export IMAGE_TAG
export AOMI_API_DOMAIN

echo "🚀 Starting backend services deployment with HTTPS..."
echo "🏷️  Using IMAGE_TAG: $IMAGE_TAG"
echo "🌐 Using DOMAIN: $AOMI_API_DOMAIN"
echo "📧 Using EMAIL: $EMAIL"
echo "📡 Services will be available via HTTPS"

# Load API keys from .env.prod
ENV_FILE="$PROJECT_ROOT/.env.prod"
if [[ -f "$ENV_FILE" ]]; then
    echo "🔑 Loading API keys from $ENV_FILE"
    export $(grep -v '^#' "$ENV_FILE" | xargs)
else
    echo "❌ No .env.prod file found at $ENV_FILE"
    echo "Please create .env.prod with your API keys before running production deployment"
    exit 1
fi

# Create necessary directories
echo "📁 Creating necessary directories..."
mkdir -p "$PROJECT_ROOT/docker/certbot/conf"
mkdir -p "$PROJECT_ROOT/docker/certbot/www"

# Stop any existing containers
echo "🛑 Stopping existing containers..."
docker compose -f "$PROJECT_ROOT/docker/docker-compose-backend.yml" down || true

# Pull latest images from GitHub Container Registry
echo "📥 Pulling images with tag: $IMAGE_TAG..."
docker pull ghcr.io/aomi-labs/product-mono/backend:$IMAGE_TAG || { echo "❌ Failed to pull backend:$IMAGE_TAG"; exit 1; }
docker pull ghcr.io/aomi-labs/product-mono/mcp:$IMAGE_TAG || { echo "❌ Failed to pull mcp:$IMAGE_TAG"; exit 1; }

# Clean up old containers
echo "🧹 Cleaning up old containers..."
docker system prune -f || true

# Check if SSL certificates already exist
if [[ -d "$PROJECT_ROOT/docker/certbot/conf/live/$AOMI_API_DOMAIN" ]]; then
    echo "✅ SSL certificates already exist for $AOMI_API_DOMAIN"
    CERT_EXISTS=true
else
    echo "🔒 SSL certificates not found, will obtain them..."
    CERT_EXISTS=false
fi

cd "$PROJECT_ROOT"

if [[ "$CERT_EXISTS" == "false" ]]; then
    # Start nginx temporarily for certificate generation
    echo "🔐 Starting temporary nginx for SSL certificate generation..."

    # Create temporary nginx config for certbot
    cat > "$PROJECT_ROOT/docker/nginx/nginx-temp.conf" << EOF
server {
    listen 80;
    server_name $AOMI_API_DOMAIN;

    location /.well-known/acme-challenge/ {
        root /var/www/certbot;
    }

    location / {
        return 200 'Setting up SSL...';
        add_header Content-Type text/plain;
    }
}
EOF

    # Start nginx with temporary config
    docker run -d \
        --name nginx-temp \
        -p 80:80 \
        -v "$PROJECT_ROOT/docker/nginx/nginx-temp.conf:/etc/nginx/conf.d/default.conf" \
        -v "$PROJECT_ROOT/docker/certbot/www:/var/www/certbot" \
        nginx:alpine

    echo "⏳ Waiting for nginx to start..."
    sleep 5

    # Obtain SSL certificate
    echo "🔒 Obtaining SSL certificate from Let's Encrypt..."
    docker run --rm \
        -v "$PROJECT_ROOT/docker/certbot/conf:/etc/letsencrypt" \
        -v "$PROJECT_ROOT/docker/certbot/www:/var/www/certbot" \
        certbot/certbot certonly \
        --webroot \
        --webroot-path=/var/www/certbot \
        --email "$EMAIL" \
        --agree-tos \
        --no-eff-email \
        -d "$AOMI_API_DOMAIN"

    # Stop temporary nginx
    docker stop nginx-temp || true
    docker rm nginx-temp || true
    rm -f "$PROJECT_ROOT/docker/nginx/nginx-temp.conf"

    if [[ ! -d "$PROJECT_ROOT/docker/certbot/conf/live/$AOMI_API_DOMAIN" ]]; then
        echo "❌ Failed to obtain SSL certificate"
        echo "Make sure your domain $AOMI_API_DOMAIN points to this server's IP address"
        exit 1
    fi

    echo "✅ SSL certificate obtained successfully"
fi

# Start backend services with SSL
echo "🚀 Starting backend services with HTTPS..."
echo "📍 Using compose file: $PROJECT_ROOT/docker/docker-compose-backend.yml"

docker compose -f docker/docker-compose-backend.yml up -d

echo "⏳ Waiting for services to start..."
sleep 15

# Check service status
echo "🔍 Checking service health..."
docker compose -f docker/docker-compose-backend.yml ps

# Test if services are responding
echo "🧪 Testing service endpoints..."

# Test HTTPS endpoint
if curl -f -s https://$AOMI_API_DOMAIN/health > /dev/null 2>&1; then
    echo "✅ Backend service is responding via HTTPS"
else
    echo "⚠️  Backend service not responding via HTTPS (this may take a moment)"
fi

# Test HTTP to HTTPS redirect
if curl -f -s -L http://$AOMI_API_DOMAIN/health > /dev/null 2>&1; then
    echo "✅ HTTP to HTTPS redirect is working"
else
    echo "⚠️  HTTP to HTTPS redirect not working"
fi

echo ""
echo "🎉 Backend deployment complete with HTTPS!"
echo ""
echo "📡 Your backend services are available at:"
echo "   🔒 Backend API:  https://$AOMI_API_DOMAIN/api/"
echo "   🔒 MCP Service:  https://$AOMI_API_DOMAIN/mcp/"
echo "   🔒 Health Check: https://$AOMI_API_DOMAIN/health"
echo "   🔒 Anvil RPC:    https://$AOMI_API_DOMAIN/anvil/"
echo ""
echo "🏷️  Deployed version: $IMAGE_TAG"
echo ""
echo "📝 Frontend configuration:"
echo "   NEXT_PUBLIC_BACKEND_URL=https://$AOMI_API_DOMAIN/api/"
echo "   NEXT_PUBLIC_ANVIL_URL=https://$AOMI_API_DOMAIN/anvil/"
echo ""
echo "🔄 SSL certificates will auto-renew every 12 hours"
echo ""
echo "📋 To monitor logs: docker compose -f docker/docker-compose-backend.yml logs -f"
echo "🛑 To stop services: docker compose -f docker/docker-compose-backend.yml down"
echo ""
echo "⚠️  Important: Make sure your domain $AOMI_API_DOMAIN points to this server's IP address"