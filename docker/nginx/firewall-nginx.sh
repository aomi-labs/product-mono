#!/usr/bin/env bash
# Hardens the public-facing NGINX proxy host by only allowing SSH + HTTP/S.
# Usage: ./firewall-nginx.sh

set -euo pipefail

if [[ $# -ne 0 ]]; then
  echo "Usage: $0" >&2
  exit 1
fi

WEB_PORTS=(80 443)
SSH_PORT=22

echo "🧱 Applying UFW firewall rules for NGINX proxy..."
echo "   SSH allowed from: anywhere"
echo "   Public web ports: ${WEB_PORTS[*]}"

# Enable ufw if not active (ignore failure if already enabled)
ufw --force enable >/dev/null 2>&1 || true

# Reset rules to avoid duplicates
ufw --force reset >/dev/null

# Default policies
ufw default deny incoming
ufw default allow outgoing

# SSH access (always open so we avoid locking ourselves out)
ufw allow "${SSH_PORT}"/tcp comment 'SSH (anywhere)'

# Allow HTTP/HTTPS for everyone
for port in "${WEB_PORTS[@]}"; do
  ufw allow "$port"/tcp comment "NGINX public port $port"
done

# Enable firewall (again) to apply the new rules
ufw --force enable

echo
echo "✅ Firewall rules applied successfully."
echo "----------------------------------------"
ufw status verbose
echo "----------------------------------------"
echo "Incoming traffic limited to SSH (anywhere) and ports ${WEB_PORTS[*]}."
