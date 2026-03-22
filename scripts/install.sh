#!/bin/bash
set -e

OWNER="frypan05"
REPO="Volt"
BINARY="volt"
INSTALL_DIR="${DIR:-$HOME/.local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
      aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
      *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-apple-darwin" ;;
      arm64)   TARGET="aarch64-apple-darwin" ;;
      *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

# Get latest release version from GitHub API
VERSION=$(curl -s "https://api.github.com/repos/$OWNER/$REPO/releases/latest" \
  | grep '"tag_name"' \
  | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$VERSION" ]; then
  echo "Failed to fetch latest version"
  exit 1
fi

FILENAME="${BINARY}-${TARGET}.tar.gz"
URL="https://github.com/$OWNER/$REPO/releases/download/$VERSION/$FILENAME"

echo "Installing $BINARY $VERSION for $TARGET..."
echo "Downloading from $URL"

# Download and extract
TMP=$(mktemp -d)
curl -sL "$URL" -o "$TMP/$FILENAME"
tar -xzf "$TMP/$FILENAME" -C "$TMP"

# Install
mkdir -p "$INSTALL_DIR"
mv "$TMP/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"
rm -rf "$TMP"

echo ""
echo "volt installed to $INSTALL_DIR/volt"

# Warn if INSTALL_DIR is not in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "WARNING: $INSTALL_DIR is not in your PATH."
  echo "Add this to your ~/.bashrc or ~/.zshrc:"
  echo ""
  echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
  echo ""
fi

echo "Done. Open a new terminal and run: volt"
