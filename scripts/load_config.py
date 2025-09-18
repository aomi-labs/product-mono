#!/usr/bin/env python3
"""
Configuration loader for forge-mcp.

- Reads environment-specific settings from `config.yaml`
- Exports service host/port variables and derived URLs
- Validates presence of required API keys (already loaded by shell scripts)
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
        
    def load_yaml_config(self) -> Tuple[Dict[str, Any], Dict[str, Any], Dict[str, Any]]:
        """Load configuration from YAML file.

        Returns a tuple of (services, settings, full_config).
        """
        if not self.config_file.exists():
            print(f"{Colors.RED}❌ Config file not found: {self.config_file}{Colors.NC}")
            sys.exit(1)
            
        with open(self.config_file, 'r', encoding='utf-8') as f:
            config = yaml.safe_load(f)
            
        # Get environment-specific config
        env_config = config.get(self.env, {})
        services = env_config.get('services', {})
        settings = env_config.get('settings', {})
        
        return services, settings, config
    
    def export_ports_only(self) -> bool:
        """Export only host/port configuration and derived URLs for shell scripts."""
        try:
            services, settings, _ = self.load_yaml_config()
        except Exception as e:
            print(f"Error loading config: {e}", file=sys.stderr)
            return False
        
        # Set defaults based on environment
        if self.env == 'production':
            default_host = '0.0.0.0'
            default_mcp_port = 5001
            default_backend_port = 8081
            default_frontend_port = 3001
        else:
            default_host = '127.0.0.1'
            default_mcp_port = 5000
            default_backend_port = 8080
            default_frontend_port = 3000
        
        # Export service configs
        configs = {
            'MCP_SERVER_HOST': self.get_service_config(services, 'mcp_server', 'host', default_host),
            'MCP_SERVER_PORT': self.get_service_config(services, 'mcp_server', 'port', default_mcp_port),
            'BACKEND_HOST': self.get_service_config(services, 'backend', 'host', default_host),
            'BACKEND_PORT': self.get_service_config(services, 'backend', 'port', default_backend_port),
            'FRONTEND_HOST': self.get_service_config(services, 'frontend', 'host', 'localhost' if self.env == 'development' else default_host),
            'FRONTEND_PORT': self.get_service_config(services, 'frontend', 'port', default_frontend_port),
            'ANVIL_HOST': self.get_service_config(services, 'anvil', 'host', '127.0.0.1'),
            'ANVIL_PORT': self.get_service_config(services, 'anvil', 'port', 8545)
        }
        
        # Export URLs
        urls = {
            'MCP_SERVER_URL': f'http://{configs["MCP_SERVER_HOST"]}:{configs["MCP_SERVER_PORT"]}',
            'BACKEND_URL': f'http://localhost:{configs["BACKEND_PORT"]}',
            'FRONTEND_URL': f'http://localhost:{configs["FRONTEND_PORT"]}',
            'ANVIL_URL': f'http://{configs["ANVIL_HOST"]}:{configs["ANVIL_PORT"]}'
        }
        
        # Print export statements
        for key, value in configs.items():
            print(f'export {key}="{value}"')
        for key, value in urls.items():
            print(f'export {key}="{value}"')
        if settings:
            for key, value in settings.items():
                print(f'export {key.upper()}="{value}"')
        
        # Export network URLs for MCP server
        self.export_network_urls()
        
        return True
    
    def get_service_config(self, services: Dict[str, Any], service_name: str, key: str, default: Optional[Any] = None) -> Any:
        """Get a configuration value for a service with a default."""
        return services.get(service_name, {}).get(key, default)
    
    def substitute_env_variables(self, text: str) -> str:
        """Substitute environment variables in text using {$VAR_NAME} syntax."""
        def replace_var(match):
            var_name = match.group(1)
            return os.getenv(var_name, f"${{{var_name}}}")  # Return original if not found
        
        return re.sub(r'\{\$([^}]+)\}', replace_var, str(text))
    
    def get_network_urls_json(self) -> str:
        """Generate network URLs JSON configuration for MCP server."""
        try:
            services, settings, config = self.load_yaml_config()
            env_config = config.get(self.env, {})
            networks = env_config.get('networks', {})
            
            if not networks:
                # Fallback to default testnet configuration
                return json.dumps({
                    "testnet": "http://127.0.0.1:8545"
                })
            
            # Build network URLs with environment variable substitution
            network_urls = {}
            for network_name, network_config in networks.items():
                if isinstance(network_config, dict) and 'url' in network_config:
                    url = self.substitute_env_variables(network_config['url'])
                    # Only include networks with valid URLs (skip those with unsubstituted variables)
                    if not url.startswith('${') and url != f"${{{network_config['url']}}}":
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
    
    def export_network_urls(self) -> None:
        """Export NETWORK_URLS environment variable for shell scripts."""
        network_urls_json = self.get_network_urls_json()
        print(f'export NETWORK_URLS=\'{network_urls_json}\'')
    
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
    
    def export_env_vars(self, services: Dict[str, Any]) -> Tuple[Dict[str, str], Dict[str, str]]:
        """Export environment variables for use by shell scripts.

        Returns a tuple of (configs, urls) where both are mapping-like dicts of str->str.
        """
        # Service configurations
        configs = {
            'MCP_SERVER_HOST': self.get_service_config(services, 'mcp_server', 'host', '127.0.0.1'),
            'MCP_SERVER_PORT': self.get_service_config(services, 'mcp_server', 'port', 5000),
            'BACKEND_HOST': self.get_service_config(services, 'backend', 'host', '0.0.0.0'),
            'BACKEND_PORT': self.get_service_config(services, 'backend', 'port', 8080),
            'FRONTEND_HOST': self.get_service_config(services, 'frontend', 'host', 'localhost'),
            'FRONTEND_PORT': self.get_service_config(services, 'frontend', 'port', 3000),
            'ANVIL_HOST': self.get_service_config(services, 'anvil', 'host', '127.0.0.1'),
            'ANVIL_PORT': self.get_service_config(services, 'anvil', 'port', 8545)
        }
        
        # Export environment variables
        for key, value in configs.items():
            os.environ[key] = str(value)
        
        # Construct URLs
        mcp_url = f"http://{configs['MCP_SERVER_HOST']}:{configs['MCP_SERVER_PORT']}"
        backend_url = f"http://localhost:{configs['BACKEND_PORT']}"
        frontend_url = f"http://localhost:{configs['FRONTEND_PORT']}"
        anvil_url = f"http://{configs['ANVIL_HOST']}:{configs['ANVIL_PORT']}"
        
        os.environ['MCP_SERVER_URL'] = mcp_url
        os.environ['BACKEND_URL'] = backend_url
        os.environ['FRONTEND_URL'] = frontend_url
        os.environ['ANVIL_URL'] = anvil_url
        
        return configs, {
            'MCP_SERVER_URL': mcp_url,
            'BACKEND_URL': backend_url,
            'FRONTEND_URL': frontend_url,
            'ANVIL_URL': anvil_url
        }
    
    def run(self) -> bool:
        """Main configuration loading logic."""
        print(f"{Colors.BLUE}🔧 Loading forge-mcp configuration...{Colors.NC}")
        print(f"{Colors.BLUE}Environment: {Colors.YELLOW}{self.env}{Colors.NC}")
        
        # Load YAML configuration
        try:
            services, _settings, _full_config = self.load_yaml_config()
        except Exception as e:
            print(f"{Colors.RED}❌ Failed to load config.yaml: {e}{Colors.NC}")
            sys.exit(1)
        
        # Export configuration as environment variables
        _configs, urls = self.export_env_vars(services)
        
        print(f"{Colors.GREEN}✅ Configuration loaded:{Colors.NC}")
        print(f"   MCP Server: {Colors.BLUE}{urls['MCP_SERVER_URL']}{Colors.NC}")
        print(f"   Backend:    {Colors.BLUE}{urls['BACKEND_URL']}{Colors.NC}")
        print(f"   Frontend:   {Colors.BLUE}{urls['FRONTEND_URL']}{Colors.NC}")
        print(f"   Anvil:      {Colors.BLUE}{urls['ANVIL_URL']}{Colors.NC}")
        
        # Show network configuration
        print()
        self.print_network_info()
        
        # Check API keys (they should already be loaded by shell script)
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
        success = loader.export_ports_only()
        sys.exit(0 if success else 1)
    else:
        success = loader.run()
        sys.exit(0 if success else 1)