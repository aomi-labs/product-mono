#!/bin/bash

# load-config.sh - Configuration loader for forge-mcp
# Loads configuration from config.yaml and validates environment variables

set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

CONFIG_FILE="$(dirname "$0")/../config.yaml"
ENV_FILE="$(dirname "$0")/../.env"
ENV_EXAMPLE="$(dirname "$0")/../.env.example"

echo -e "${BLUE}🔧 Loading forge-mcp configuration...${NC}"

# Simple function to extract values from YAML
get_yaml_value() {
    local section=$1
    local key=$2
    if [[ -f "$CONFIG_FILE" ]]; then
        # More reliable approach: extract the specific value by path
        awk -v section="$section" -v key="$key" '
        BEGIN { in_section = 0; in_services = 0 }
        /^services:/ { in_services = 1; next }
        in_services && /^[[:space:]]*[a-z_]+:/ {
            current_section = $0
            gsub(/:.*/, "", current_section)
            gsub(/^[[:space:]]*/, "", current_section)
            if (current_section == section) {
                in_section = 1
            } else {
                in_section = 0
            }
            next
        }
        in_section && $0 ~ "^[[:space:]]*" key ":" {
            value = $0
            gsub(/^[[:space:]]*[^:]*:[[:space:]]*/, "", value)
            gsub(/^"/, "", value)
            gsub(/"$/, "", value)
            print value
            exit
        }
        /^[a-z]/ && !/^services:/ { in_services = 0; in_section = 0 }
        ' "$CONFIG_FILE"
    fi
}

# Load configuration with fallback to defaults
MCP_SERVER_HOST=${MCP_SERVER_HOST:-$(get_yaml_value "mcp_server" "host")}
MCP_SERVER_PORT=${MCP_SERVER_PORT:-$(get_yaml_value "mcp_server" "port")}
BACKEND_HOST=${BACKEND_HOST:-$(get_yaml_value "backend" "host")}
BACKEND_PORT=${BACKEND_PORT:-$(get_yaml_value "backend" "port")}
FRONTEND_PORT=${FRONTEND_PORT:-$(get_yaml_value "frontend" "port")}
ANVIL_HOST=${ANVIL_HOST:-$(get_yaml_value "anvil" "host")}
ANVIL_PORT=${ANVIL_PORT:-$(get_yaml_value "anvil" "port")}

# Set defaults if nothing found
export MCP_SERVER_HOST=${MCP_SERVER_HOST:-"127.0.0.1"}
export MCP_SERVER_PORT=${MCP_SERVER_PORT:-5000}
export BACKEND_HOST=${BACKEND_HOST:-"0.0.0.0"}
export BACKEND_PORT=${BACKEND_PORT:-8080}
export FRONTEND_PORT=${FRONTEND_PORT:-3000}
export ANVIL_HOST=${ANVIL_HOST:-"127.0.0.1"}
export ANVIL_PORT=${ANVIL_PORT:-8545}

# Construct URLs
export MCP_SERVER_URL="http://${MCP_SERVER_HOST}:${MCP_SERVER_PORT}"
export BACKEND_URL="http://localhost:${BACKEND_PORT}"
export FRONTEND_URL="http://localhost:${FRONTEND_PORT}"
export ANVIL_URL="http://${ANVIL_HOST}:${ANVIL_PORT}"

echo -e "${GREEN}✅ Configuration loaded:${NC}"
echo -e "   MCP Server: ${BLUE}${MCP_SERVER_URL}${NC}"
echo -e "   Backend:    ${BLUE}${BACKEND_URL}${NC}"
echo -e "   Frontend:   ${BLUE}${FRONTEND_URL}${NC}"
echo -e "   Anvil:      ${BLUE}${ANVIL_URL}${NC}"

# Check for required API keys
echo -e "${BLUE}🔍 Checking API keys...${NC}"

check_env_var() {
    local var_name=$1
    local required=$2
    local description=$3
    
    # Use eval for indirect variable access
    local var_value
    eval "var_value=\$$var_name"
    
    if [[ -n "$var_value" ]]; then
        echo -e "   ${GREEN}✅ ${var_name}${NC}: ${description}"
        return 0
    elif [[ "$required" == "true" ]]; then
        echo -e "   ${RED}❌ ${var_name}${NC}: ${description} (REQUIRED)"
        return 1
    else
        echo -e "   ${YELLOW}⚠️  ${var_name}${NC}: ${description} (optional - not set)"
        return 0
    fi
}

# Check if environment variables are set, if not try to load from .env
missing_required=false

if [[ -z "$ANTHROPIC_API_KEY" ]] && [[ -f "$ENV_FILE" ]]; then
    echo -e "${YELLOW}📄 Loading environment variables from .env file...${NC}"
    set -a  # automatically export all variables
    source "$ENV_FILE"
    set +a
fi

# Check all API keys
if ! check_env_var "ANTHROPIC_API_KEY" "true" "Anthropic Claude API access"; then
    missing_required=true
fi

check_env_var "BRAVE_SEARCH_API_KEY" "false" "Web search capabilities"
check_env_var "ETHERSCAN_API_KEY" "false" "Blockchain data and contract ABIs"
check_env_var "ZEROX_API_KEY" "false" "Token swap functionality via 0x Protocol"

# Handle missing required variables
if [[ "$missing_required" == "true" ]]; then
    echo ""
    echo -e "${RED}❌ ERROR: Required environment variables are missing!${NC}"
    echo ""
    echo -e "${YELLOW}🔧 To fix this:${NC}"
    
    if [[ ! -f "$ENV_FILE" ]]; then
        echo -e "1. Copy the example environment file:"
        echo -e "   ${BLUE}cp .env.example .env${NC}"
        echo ""
    fi
    
    echo -e "2. Edit the .env file and add your API keys:"
    echo -e "   ${BLUE}nano .env${NC}"
    echo ""
    echo -e "3. Or set the environment variable directly:"
    echo -e "   ${BLUE}export ANTHROPIC_API_KEY=\"your-api-key-here\"${NC}"
    echo ""
    echo -e "Get your Anthropic API key from: ${BLUE}https://console.anthropic.com/${NC}"
    
    return 1
fi

echo -e "${GREEN}✅ All required environment variables are set${NC}"
