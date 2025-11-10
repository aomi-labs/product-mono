#!/usr/bin/env bash
# firewall-ufw.sh
# Usage: ./firewall-ufw.sh <PROXY_IP>
#
# Allows:
#   - SSH (22) from anywhere
#   - Ports 8081, 5001, 8545 only from the given proxy IP
# Denies all other inbound connections.

set -e

if [ $# -ne 1 ]; then
  echo "Usage: $0 <PROXY_IP>"
  exit 1
fi

PROXY_IP="$1"
PORTS=(8081 5001 8545)

echo "🧱 Applying UFW firewall rules..."
echo "   Proxy IP: $PROXY_IP"
echo "   Allowed backend ports: ${PORTS[*]}"

# Enable ufw if not active
ufw --force enable >/dev/null 2>&1 || true

# Reset all rules
ufw --force reset >/dev/null

# Default policies
ufw default deny incoming
ufw default allow outgoing

# Allow SSH from everywhere
ufw allow 22/tcp comment 'SSH for all'

# Allow proxy IP for backend ports
for port in "${PORTS[@]}"; do
  ufw allow from "$PROXY_IP" to any port "$port" proto tcp comment "proxy to $port"
done

# Enable firewall
ufw --force enable

echo
echo "✅ Firewall rules applied successfully."
echo "----------------------------------------"
ufw status verbose
echo "----------------------------------------"
echo "SSH open to all; ports ${PORTS[*]} allowed only from $PROXY_IP."
echo "All other inbound traffic is denied."
