#!/bin/sh
# Install release-ratchet
# Usage: curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh

set -e

REPO="binary-birthday/release-ratchet"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  OS="unknown-linux-gnu" ;;
  darwin) OS="apple-darwin" ;;
  *)      echo "Unsupported OS: $OS"; exit 1 ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)             echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

TARGET="${ARCH}-${OS}"
BINARY="release-ratchet-${TARGET}"

# Get latest version
if [ -z "$VERSION" ]; then
  VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
  if [ -z "$VERSION" ]; then
    echo "Failed to determine latest version"
    exit 1
  fi
fi

URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}"

echo "Installing release-ratchet ${VERSION} (${TARGET})..."
echo "  from: ${URL}"
echo "  to:   ${INSTALL_DIR}/release-ratchet"

# Download
TMP=$(mktemp)
curl -fsSL "$URL" -o "$TMP"
chmod +x "$TMP"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP" "${INSTALL_DIR}/release-ratchet"
else
  sudo mv "$TMP" "${INSTALL_DIR}/release-ratchet"
fi

echo "Installed release-ratchet ${VERSION}"
release-ratchet --version
