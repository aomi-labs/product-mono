#!/usr/bin/env python3
"""Configuration helper for dev/prod workflows.

This script assumes API keys are already present in the environment (sourced
from .env files by shell scripts). It computes derived values like service ports
and MCP network JSON using config.yaml.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, Iterable, Optional, Tuple

try:
    import yaml
except ModuleNotFoundError:  # pragma: no cover - friendly message
    sys.stderr.write("PyYAML is required. Run 'pip install -r requirements.txt' to install dependencies.\n")
    sys.exit(1)

ROOT = Path(__file__).resolve().parent.parent
CONFIG_PATH = ROOT / "config.yaml"
ENV_LABELS = {"dev": "development", "development": "development",
              "prod": "production", "production": "production"}

PLACEHOLDER_PATTERN = re.compile(r"\$\{([^}]+)\}|\{\$([^}]+)\}")
OPTIONAL_KEYS = {
    "ETHERSCAN_API_KEY",
    "ZEROX_API_KEY",
}


def load_config(env_key: str) -> Dict:
    if not CONFIG_PATH.exists():
        raise FileNotFoundError(f"config.yaml not found at {CONFIG_PATH}")
    with CONFIG_PATH.open("r", encoding="utf-8") as handle:
        raw = yaml.safe_load(handle) or {}
    section = ENV_LABELS.get(env_key, env_key)
    return raw.get(section, {})


def substitute_placeholders(value: str) -> str:
    def repl(match: re.Match[str]) -> str:
        var = match.group(1) or match.group(2)
        return os.getenv(var, match.group(0))

    return PLACEHOLDER_PATTERN.sub(repl, value)


def print_check_status(name: str, present: bool, optional: bool = False) -> None:
    status = "✅" if present else ("⚠️" if optional else "❌")
    color = "\033[32m" if present else ("\033[33m" if optional else "\033[31m")
    reset = "\033[0m"
    label = "optional" if optional else "required"
    print(f"{color}{status} {name}{reset} ({label})")


def resolve_service_exports(env_key: str, config: Dict) -> Dict[str, str]:
    services = config.get("services", {})
    exports = {}
    overrides = {
        "MCP_SERVER_HOST": ("mcp_server", "host", "127.0.0.1"),
        "MCP_SERVER_PORT": ("mcp_server", "port", "5000"),
        "BACKEND_HOST": ("backend", "host", "0.0.0.0"),
        "BACKEND_PORT": ("backend", "port", "8080"),
        "FRONTEND_HOST": ("frontend", "host", "localhost"),
        "FRONTEND_PORT": ("frontend", "port", "3000"),
        "ANVIL_HOST": ("anvil", "host", "127.0.0.1"),
        "ANVIL_PORT": ("anvil", "port", "8545"),
    }

    for env_var, (service, field, fallback) in overrides.items():
        value = services.get(service, {}).get(field, fallback)
        exports[env_var] = str(value)

    if env_key.startswith("prod"):
        return {}

    return exports


def alchemy_key_for(network: str) -> Optional[str]:
    specific = os.getenv(f"{network.upper()}_ALCHEMY_API_KEY")
    if specific:
        return specific
    return os.getenv("ALCHEMY_API_KEY")


def build_networks(env_key: str, config: Dict) -> Dict[str, str]:
    networks = config.get("networks", {})
    resolved: Dict[str, str] = {}

    for name, entry in networks.items():
        raw = entry.get("url") if isinstance(entry, dict) else str(entry)
        url = substitute_placeholders(str(raw))

        if env_key.startswith("prod") and name != "testnet":
            key = alchemy_key_for(name)
            if not key:
                # Skip networks without credentials in production
                continue
            base = url.rstrip("/")
            url = f"{base}/{key}"

        if "{" in url and "}" in url:
            # Leave unresolved placeholders out of the final result
            continue
        resolved[name] = url

    if not resolved.get("testnet"):
        resolved["testnet"] = "http://127.0.0.1:8545" if env_key.startswith("dev") else "http://anvil:8545"

    return resolved


def extract_placeholder_vars(config: Dict) -> Iterable[str]:
    vars_found = set()
    networks = config.get("networks", {})
    for node in networks.values():
        raw = node.get("url") if isinstance(node, dict) else str(node)
        for match in PLACEHOLDER_PATTERN.finditer(str(raw)):
            var = match.group(1) or match.group(2)
            vars_found.add(var)
    return vars_found


def check_required_keys(env_key: str, config: Dict) -> Tuple[Iterable[str], Iterable[str]]:
    required = {"ANTHROPIC_API_KEY", "BRAVE_SEARCH_API_KEY", "ETHERSCAN_API_KEY"}
    if env_key.startswith("prod"):
        required.add("ALCHEMY_API_KEY")

    required.update(extract_placeholder_vars(config))

    print("🔍 Checking environment variables")

    missing_required: list[str] = []
    for key in sorted(required):
        present = bool(os.getenv(key))
        if not present:
            missing_required.append(key)
        print_check_status(key, present, optional=False)

    missing_optional: list[str] = []
    for key in sorted(OPTIONAL_KEYS):
        if key in required:
            continue
        present = bool(os.getenv(key))
        if not present:
            missing_optional.append(key)
        print_check_status(key, present, optional=True)

    return missing_required, missing_optional


def main(argv: Optional[Iterable[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Compute derived configuration")
    parser.add_argument("env", nargs="?", default="dev", help="Environment (dev|prod)")
    parser.add_argument("--chain-json", action="store_true", dest="network", help="Print network JSON")
    parser.add_argument("--export-network-env", action="store_true", dest="export_env", help="Print shell exports")
    parser.add_argument("--check-keys", action="store_true", dest="check_keys", help="Ensure required API keys exist")

    args = parser.parse_args(list(argv) if argv is not None else None)
    env_key = args.env.lower()
    if env_key not in ENV_LABELS:
        sys.stderr.write("Environment must be dev or prod\n")
        return 1

    config = load_config(env_key)

    if args.check_keys:
        missing_required, missing_optional = check_required_keys(env_key, config)
        if missing_optional:
            sys.stderr.write("Warning: optional API keys missing: " + ", ".join(missing_optional) + "\n")
        if missing_required:
            sys.stderr.write("Missing required environment variables: " + ", ".join(missing_required) + "\n")
            return 2

    if args.export_env:
        exports = resolve_service_exports(env_key, config)
        for key, value in exports.items():
            print(f"export {key}={value}")
        return 0

    if args.network:
        networks = build_networks(env_key, config)
        print(json.dumps(networks, separators=(",", ":")))
        return 0

    # # Default: check keys and display summary
    # missing_required, missing_optional = check_required_keys(env_key, config)
    # if missing_optional:
    #     sys.stderr.write("Warning: optional API keys missing: " + ", ".join(missing_optional) + "\n")
    # if missing_required:
    #     sys.stderr.write("Missing required environment variables: " + ", ".join(missing_required) + "\n")
    #     return 2

    networks = build_networks(env_key, config)
    print("Configured networks:")
    for name, url in networks.items():
        print(f"  🪐 {name}: {url}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
