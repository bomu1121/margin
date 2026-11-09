#!/usr/bin/env bash
set -euo pipefail

# Yebian sync server installer (Ubuntu/Debian/CentOS/TencentOS).
# Run as root after uploading the repository:
#   sudo bash sync-server/deploy/install.sh /opt/yebian-sync-src
#
# Bind order:
#   1. BIND_OVERRIDE if set (e.g. 127.0.0.1:8787 when Caddy terminates TLS)
#   2. the Tailscale IPv4 when tailscale is up (never exposed on the public NIC)
#   3. 127.0.0.1:8787

SRC="${1:-/tmp/yebian-sync-src}"
APP_USER="yebian-sync"
DATA_DIR="/var/lib/yebian-sync"
BIN="/usr/local/bin/yebian-sync-server"
ENV_FILE="/etc/default/yebian-sync"

if [[ $EUID -ne 0 ]]; then
  echo "run as root" >&2
  exit 1
fi
if [[ ! -f "$SRC/sync-server/Cargo.toml" ]]; then
  echo "source not found at $SRC" >&2
  exit 1
fi

mem_kb=$(awk '/MemTotal/{print $2}' /proc/meminfo)
if (( mem_kb < 2 * 1024 * 1024 )) && [[ ! -f /swapfile ]]; then
  echo "adding 2G swap for the release build"
  fallocate -l 2G /swapfile 2>/dev/null || dd if=/dev/zero of=/swapfile bs=1M count=2048 status=none
  chmod 600 /swapfile
  mkswap /swapfile >/dev/null
  swapon /swapfile
  grep -q '^/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
fi

if command -v apt-get >/dev/null; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -y
  apt-get install -y build-essential pkg-config ca-certificates curl sqlite3
elif command -v dnf >/dev/null; then
  dnf install -y gcc gcc-c++ make pkgconf-pkg-config ca-certificates curl sqlite
elif command -v yum >/dev/null; then
  yum install -y gcc gcc-c++ make pkgconfig ca-certificates curl sqlite
fi

if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1090
. "$HOME/.cargo/env" 2>/dev/null || true

cd "$SRC/sync-server"
cargo build --release

id -u "$APP_USER" >/dev/null 2>&1 || \
  useradd --system --home "$DATA_DIR" --shell /usr/sbin/nologin "$APP_USER"
install -d -o "$APP_USER" -g "$APP_USER" -m 750 "$DATA_DIR"
install -m 755 target/release/yebian-sync-server "$BIN"

BIND="${BIND_OVERRIDE:-}"
if [[ -z "$BIND" ]] && command -v tailscale >/dev/null; then
  ts_ip=$(tailscale ip -4 2>/dev/null | head -n1 || true)
  [[ -n "$ts_ip" ]] && BIND="${ts_ip}:8787"
fi
if [[ -z "$BIND" ]]; then
  BIND="127.0.0.1:8787"
fi
printf 'YEBIAN_SYNC_BIND=%s\n' "$BIND" > "$ENV_FILE"
chmod 644 "$ENV_FILE"

cat > /etc/systemd/system/yebian-sync.service <<'UNIT'
[Unit]
Description=Yebian sync relay
After=network-online.target tailscaled.service
Wants=network-online.target

[Service]
User=yebian-sync
Group=yebian-sync
EnvironmentFile=/etc/default/yebian-sync
ExecStart=/usr/local/bin/yebian-sync-server --db /var/lib/yebian-sync/sync.sqlite --blobs /var/lib/yebian-sync/blobs --bind ${YEBIAN_SYNC_BIND}
Restart=always
RestartSec=3
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/yebian-sync
CapabilityBoundingSet=
AmbientCapabilities=

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable --now yebian-sync
sleep 1
systemctl --no-pager status yebian-sync | head -n 12 || true

echo
echo "installed"
echo "bind: $BIND"
echo "create a vault (token is printed once):"
echo "  sudo -u $APP_USER $BIN create-vault default --db $DATA_DIR/sync.sqlite --blobs $DATA_DIR/blobs"
