#!/usr/bin/env python3
"""
Configuration loader for forge-mcp.

- Reads environment-specific settings from `config.yaml`
- Loads environment variables from .env files
- Exports service host/port variables and derived URLs
- Validates presence of required API keys
"""

from pathlib import Path
from typing import Any, Dict, Optional, Tuple
import os
import sys
import yaml
import json
import re

class Colors:
    """ANSI color codes for terminal output."""
    GREEN = '\033[0;32m'
    BLUE = '\033[0;34m'
    YELLOW = '\033[1;33m'
    RED = '\033[0;31m'
    NC = '\033[0m'  # No Color

class ConfigLoader:
    def __init__(self, environment: str = 'development') -> None:
        self.script_dir = Path(__file__).parent
        self.project_root = self.script_dir.parent
        self.config_file = self.project_root / 'config.yaml'

        # Normalize environment from argument
        self.env = (
            'development' if environment == 'dev'
            else 'production' if environment == 'prod'
            else environment
        )

        # Load environment variables from .env file
        self.load_env_file()

    def load_env_file(self) -> None:
        """Load environment variables from .env.{environment} file."""
        env_file = self.project_root / f'.env.{self.env[:3]}'  # .env.dev or .env.pro

        if env_file.exists():
            with open(env_file, 'r', encoding='utf-8') as f:
                for line in f:
                    line = line.strip()
                    # Skip comments and empty lines
                    if line and not line.startswith('#') and '=' in line:
                        key, value = line.split('=', 1)
                        # Remove quotes if present
                        value = value.strip('"\'')
                        # Only set if not already in environment (don't override existing vars)
                        if key not in os.environ:
                            os.environ[key] = value

    def load_yaml_config(self) -> Tuple[Dict[str, Any], Dict[str, Any]]:
        """Load configuration from YAML file. Returns (services, settings)."""
        if not self.config_file.exists():
            print(f"{Colors.RED}❌ Config file not found: {self.config_file}{Colors.NC}")
            sys.exit(1)

        with open(self.config_file, 'r', encoding='utf-8') as f:
            config = yaml.safe_load(f)

        # Get environment-specific config
        env_config = config.get(self.env, {})
        services = env_config.get('services', {})
        settings = env_config.get('settings', {})

        return services, settings

    def get_service_defaults(self) -> Dict[str, Any]:
        """Get default service configurations based on environment."""
        if self.env == 'production':
            return {
                'host': '0.0.0.0',
                'mcp_port': 5001,
                'backend_port': 8081,
                'frontend_port': 3001,
                'anvil_host': '127.0.0.1',
                'anvil_port': 8545
            }
        else:
            return {
                'host': '127.0.0.1',
                'mcp_port': 5000,
                'backend_port': 8080,
                'frontend_port': 3000,
                'anvil_host': '127.0.0.1',
                'anvil_port': 8545
            }

    def get_service_config(self, services: Dict[str, Any], service_name: str, key: str, default: Any) -> Any:
        """Get a configuration value for a service with a default."""
        return services.get(service_name, {}).get(key, default)

    def build_service_configs(self, services: Dict[str, Any]) -> Dict[str, str]:
        """Build service configuration dictionary."""
        defaults = self.get_service_defaults()

        configs = {
            'MCP_SERVER_HOST': self.get_service_config(services, 'mcp_server', 'host', defaults['host']),
            'MCP_SERVER_PORT': self.get_service_config(services, 'mcp_server', 'port', defaults['mcp_port']),
            'BACKEND_HOST': self.get_service_config(services, 'backend', 'host', defaults['host']),
            'BACKEND_PORT': self.get_service_config(services, 'backend', 'port', defaults['backend_port']),
            'FRONTEND_HOST': self.get_service_config(services, 'frontend', 'host', 'localhost' if self.env == 'development' else defaults['host']),
            'FRONTEND_PORT': self.get_service_config(services, 'frontend', 'port', defaults['frontend_port']),
            'ANVIL_HOST': self.get_service_config(services, 'anvil', 'host', defaults['anvil_host']),
            'ANVIL_PORT': self.get_service_config(services, 'anvil', 'port', defaults['anvil_port'])
        }

        return configs

    def build_urls(self, configs: Dict[str, str]) -> Dict[str, str]:
        """Build service URLs from configurations."""
        return {
            'MCP_SERVER_URL': f'http://{configs["MCP_SERVER_HOST"]}:{configs["MCP_SERVER_PORT"]}',
            'BACKEND_URL': f'http://localhost:{configs["BACKEND_PORT"]}',
            'FRONTEND_URL': f'http://localhost:{configs["FRONTEND_PORT"]}',
            'ANVIL_URL': f'http://{configs["ANVIL_HOST"]}:{configs["ANVIL_PORT"]}'
        }

    def substitute_env_variables(self, text: str) -> str:
        """Substitute environment variables in text using {$VAR_NAME} syntax."""
        def replace_var(match):
            var_name = match.group(1)
            return os.getenv(var_name, f"${{{var_name}}}")  # Return original if not found

        return re.sub(r'\{\$([^}]+)\}', replace_var, str(text))

    def get_network_urls_json(self) -> str:
        """Generate network URLs JSON configuration for MCP server."""
        try:
            services, settings = self.load_yaml_config()

            # Load full config to get networks
            with open(self.config_file, 'r', encoding='utf-8') as f:
                config = yaml.safe_load(f)

            env_config = config.get(self.env, {})
            networks = env_config.get('networks', {})

            if not networks:
                # Fallback to default testnet configuration
                return json.dumps({"testnet": "http://127.0.0.1:8545"})

            # Build network URLs with environment variable substitution
            network_urls = {}
            for network_name, network_config in networks.items():
                if isinstance(network_config, dict) and 'url' in network_config:
                    url = self.substitute_env_variables(network_config['url'])
                    # Only include networks with valid URLs (skip those with unsubstituted variables)
                    if not url.startswith('${'):
                        network_urls[network_name] = url
                elif isinstance(network_config, str):
                    # Handle simple string format
                    url = self.substitute_env_variables(network_config)
                    if not url.startswith('${'):
                        network_urls[network_name] = url

            # Ensure we always have at least testnet
            if 'testnet' not in network_urls:
                network_urls['testnet'] = "http://127.0.0.1:8545"

            return json.dumps(network_urls, separators=(',', ':'))  # Compact JSON

        except Exception as e:
            print(f"Warning: Failed to parse networks config: {e}", file=sys.stderr)
            return json.dumps({"testnet": "http://127.0.0.1:8545"})

    def print_network_info(self) -> None:
        """Print information about configured networks."""
        try:
            network_urls = json.loads(self.get_network_urls_json())
            print(f"{Colors.BLUE}🌐 Configured Networks:{Colors.NC}")
            for name, url in network_urls.items():
                # Mask sensitive parts of URLs for display
                display_url = re.sub(r'([a-zA-Z0-9]{8})[a-zA-Z0-9]{24,}', r'\1...', url)
                print(f"   {Colors.GREEN}✅ {name}:{Colors.NC} {display_url}")
        except Exception as e:
            print(f"   {Colors.YELLOW}⚠️  Network config error: {e}{Colors.NC}")

    def check_api_keys(self) -> bool:
        """Check and validate API keys."""
        api_keys = [
            ('ANTHROPIC_API_KEY', True, 'Anthropic Claude API access'),
            ('BRAVE_SEARCH_API_KEY', False, 'Web search capabilities'),
            ('ETHERSCAN_API_KEY', False, 'Blockchain data and contract ABIs'),
            ('ZEROX_API_KEY', False, 'Token swap functionality via 0x Protocol')
        ]

        missing_required = False

        for var_name, required, description in api_keys:
            value = os.getenv(var_name)

            if value:
                print(f"   {Colors.GREEN}✅ {var_name}{Colors.NC}: {description}")
            elif required:
                print(f"   {Colors.RED}❌ {var_name}{Colors.NC}: {description} (REQUIRED)")
                missing_required = True
            else:
                print(f"   {Colors.YELLOW}⚠️  {var_name}{Colors.NC}: {description} (optional - not set)")

        return not missing_required

    def print_config_error(self) -> None:
        """Print configuration error help."""
        print(f"\n{Colors.RED}❌ ERROR: Required environment variables are missing!{Colors.NC}")
        print(f"\n{Colors.YELLOW}🔧 To fix this:{Colors.NC}")
        print("1. Copy the template file (if you haven't created one):")
        print(f"   {Colors.BLUE}cp .env.template {self.project_root}/.env.{self.env}{Colors.NC}")
        print()
        print("2. Edit the environment file and add your API keys:")
        print(f"   {Colors.BLUE}nano .env.{self.env}{Colors.NC}")
        print()
        print("3. Or set the environment variable directly:")
        print(f"   {Colors.BLUE}export ANTHROPIC_API_KEY=\"your-api-key-here\"{Colors.NC}")
        print()
        print(f"Get your Anthropic API key from: {Colors.BLUE}https://console.anthropic.com/{Colors.NC}")

    def export_only(self) -> bool:
        """Export only host/port configuration and derived URLs for shell scripts."""
        try:
            services, settings = self.load_yaml_config()
        except Exception as e:
            print(f"Error loading config: {e}", file=sys.stderr)
            return False

        configs = self.build_service_configs(services)
        urls = self.build_urls(configs)

        # Print export statements
        for key, value in configs.items():
            print(f'export {key}="{value}"')
        for key, value in urls.items():
            print(f'export {key}="{value}"')
        if settings:
            for key, value in settings.items():
                print(f'export {key.upper()}="{value}"')

        # Export network URLs for MCP server
        network_urls_json = self.get_network_urls_json()
        print(f'export NETWORK_URLS=\'{network_urls_json}\'')

        return True

    def run(self) -> bool:
        """Main configuration loading logic with full output."""
        print(f"{Colors.BLUE}🔧 Loading forge-mcp configuration...{Colors.NC}")
        print(f"{Colors.BLUE}Environment: {Colors.YELLOW}{self.env}{Colors.NC}")

        # Load YAML configuration
        try:
            services, settings = self.load_yaml_config()
        except Exception as e:
            print(f"{Colors.RED}❌ Failed to load config.yaml: {e}{Colors.NC}")
            sys.exit(1)

        # Build configurations
        configs = self.build_service_configs(services)
        urls = self.build_urls(configs)

        # Export to environment
        for key, value in configs.items():
            os.environ[key] = str(value)
        for key, value in urls.items():
            os.environ[key] = value

        # Print configuration summary
        print(f"{Colors.GREEN}✅ Configuration loaded:{Colors.NC}")
        print(f"   MCP Server: {Colors.BLUE}{urls['MCP_SERVER_URL']}{Colors.NC}")
        print(f"   Backend:    {Colors.BLUE}{urls['BACKEND_URL']}{Colors.NC}")
        print(f"   Frontend:   {Colors.BLUE}{urls['FRONTEND_URL']}{Colors.NC}")
        print(f"   Anvil:      {Colors.BLUE}{urls['ANVIL_URL']}{Colors.NC}")

        # Show network configuration
        print()
        self.print_network_info()

        # Check API keys
        print(f"{Colors.BLUE}🔍 Checking API keys...{Colors.NC}")

        if not self.check_api_keys():
            self.print_config_error()
            sys.exit(1)

        print(f"{Colors.GREEN}✅ All required environment variables are set{Colors.NC}")
        return True

if __name__ == '__main__':
    import argparse

    parser = argparse.ArgumentParser(description='Load forge-mcp configuration')
    parser.add_argument('environment', nargs='?', default='dev',
                       choices=['dev', 'prod', 'development', 'production'],
                       help='Environment to load (dev/prod)')
    parser.add_argument('--export-only', action='store_true',
                       help='Only export port configuration (no validation output)')
    parser.add_argument('--network-urls-only', action='store_true',
                       help='Only output network URLs JSON (for MCP server)')

    args = parser.parse_args()

    loader = ConfigLoader(args.environment)

    if args.network_urls_only:
        # Just output the JSON without any extra formatting
        print(loader.get_network_urls_json())
        sys.exit(0)
    elif args.export_only:
        success = loader.export_only()
        sys.exit(0 if success else 1)
    else:
        success = loader.run()
        sys.exit(0 if success else 1)