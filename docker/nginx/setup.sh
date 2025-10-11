#!/bin/bash

# Usage: ./setup.sh [domain] [email]
# Example: ./setup.sh api.foameo.ai admin@foameo.ai
# If no arguments provided, will use values from .env file

set -e

# Source .env file if it exists
if [ -f .env ]; then
    set -a
    source .env
    set +a
fi

# Get arguments - use .env values as defaults if available
DOMAIN="${1:-${AOMI_API_DOMAIN:-api.aomi.dev}}"
EMAIL="${2:-admin@example.com}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Certificate paths
CERT_PATH="../certbot/conf/live/${DOMAIN}/fullchain.pem"
KEY_PATH="../certbot/conf/live/${DOMAIN}/privkey.pem"

echo -e "${GREEN}SSL Certificate Setup for ${DOMAIN}${NC}"
echo "======================================="

# Check if certificates already exist
if [ -f "${CERT_PATH}" ] && [ -f "${KEY_PATH}" ]; then
    echo -e "${YELLOW}✓ SSL certificates already exist for ${DOMAIN}${NC}"
    echo "  Certificate: ${CERT_PATH}"
    echo "  Private key: ${KEY_PATH}"
    
    # Check certificate expiration
    if command -v openssl &> /dev/null; then
        EXPIRY=$(openssl x509 -enddate -noout -in "${CERT_PATH}" | cut -d= -f2)
        echo -e "  Expires: ${EXPIRY}"
    fi
    
    echo -e "\n${GREEN}Starting nginx...${NC}"
    docker-compose up -d
    exit 0
fi

echo -e "${YELLOW}No SSL certificates found. Obtaining new certificates...${NC}"

# Create directories if they don't exist
echo "Creating certificate directories..."
mkdir -p ../certbot/conf
mkdir -p ../certbot/www
mkdir -p logs

# Check if nginx is running and stop it
if docker ps | grep -q "aomi-api-proxy"; then
    echo -e "${YELLOW}Stopping nginx container...${NC}"
    docker-compose down
fi

# Check if port 80 is available
if lsof -Pi :80 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${RED}Error: Port 80 is already in use${NC}"
    echo "Please stop any services using port 80 and try again"
    exit 1
fi

# Request certificate using standalone mode
echo -e "\n${GREEN}Requesting SSL certificate from Let's Encrypt...${NC}"
echo "Domain: ${DOMAIN}"
echo "Email: ${EMAIL}"
echo ""

docker run --rm \
  -v "$PWD/../certbot/conf:/etc/letsencrypt" \
  -v "$PWD/../certbot/www:/var/www/certbot" \
  -p 80:80 \
  certbot/certbot certonly \
    --standalone \
    -d "${DOMAIN}" \
    -m "${EMAIL}" \
    --agree-tos \
    --no-eff-email \
    --non-interactive

# Check if certificate was created successfully
if [ -f "${CERT_PATH}" ] && [ -f "${KEY_PATH}" ]; then
    echo -e "\n${GREEN}✓ SSL certificates obtained successfully!${NC}"
    echo "  Certificate: ${CERT_PATH}"
    echo "  Private key: ${KEY_PATH}"
    
    # Start nginx with the new certificates
    echo -e "\n${GREEN}Starting nginx with SSL enabled...${NC}"
    docker-compose up -d
    
    # Wait a moment for nginx to start
    sleep 3
    
    # Check if nginx started successfully
    if docker ps | grep -q "aomi-api-proxy"; then
        echo -e "\n${GREEN}✓ Nginx is running successfully!${NC}"
        echo ""
        echo "You can now access your API at:"
        echo "  https://${DOMAIN}/health"
        echo ""
        echo "To check nginx logs:"
        echo "  docker-compose logs -f api-proxy"
    else
        echo -e "\n${RED}Warning: Nginx may not have started properly${NC}"
        echo "Check logs with: docker-compose logs api-proxy"
    fi
else
    echo -e "\n${RED}Error: Failed to obtain SSL certificates${NC}"
    echo "Please check the output above for errors"
    exit 1
fi

echo -e "\n${GREEN}Setup complete!${NC}"