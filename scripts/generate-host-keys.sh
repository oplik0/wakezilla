#!/usr/bin/env bash
# Generate SSH host keys for the Raspberry Pi
# These keys are used by sops-nix to decrypt secrets on first boot

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KEYS_DIR="$SCRIPT_DIR/../rpi-system/keys"

mkdir -p "$KEYS_DIR"

# Generate ed25519 host key if it doesn't exist
if [ -f "$KEYS_DIR/ssh_host_ed25519_key" ]; then
    echo "SSH host key already exists at $KEYS_DIR/ssh_host_ed25519_key"
    echo "Delete it first if you want to regenerate."
    exit 1
fi

echo "Generating SSH host key..."
ssh-keygen -t ed25519 -f "$KEYS_DIR/ssh_host_ed25519_key" -N "" -C "wol-rpi-host-key"

echo ""
echo "SSH host key generated successfully!"
echo ""
echo "Next steps:"
echo "1. Get the age public key for .sops.yaml:"
echo "   cat $KEYS_DIR/ssh_host_ed25519_key.pub | nix run nixpkgs#ssh-to-age"
echo ""
echo "2. Add the age key to rpi-system/.sops.yaml"
echo ""
echo "3. Create/edit secrets:"
echo "   cd rpi-system && nix run nixpkgs#sops secrets/secrets.yaml"
echo ""
echo "SECURITY NOTE: The private key ($KEYS_DIR/ssh_host_ed25519_key)"
echo "is gitignored but MUST be kept secure. Anyone with this key can"
echo "decrypt all secrets in the repository."
