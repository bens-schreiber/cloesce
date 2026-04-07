#!/bin/sh
set -e

REPO="bens-schreiber/cloesce"
BINARY="cloesce"

# Detect OS
case "$(uname -s)" in
  Linux)  OS="linux"  ;;
  Darwin) OS="macos"  ;;
  *)
    echo "Unsupported operating system: $(uname -s)" >&2
    exit 1
    ;;
esac

# Detect architecture
case "$(uname -m)" in
  x86_64|amd64)  ARCH="x86_64"  ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

ASSET_NAME="cloesce-compiler-${ARCH}-${OS}"

echo "Fetching latest Cloesce release..."
LATEST_TAG="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | head -1 \
  | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"

if [ -z "$LATEST_TAG" ]; then
  echo "Failed to determine the latest release tag." >&2
  exit 1
fi

echo "Latest release: ${LATEST_TAG}"

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}.tar.gz"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading ${ASSET_NAME}.tar.gz..."
curl -fsSL "$DOWNLOAD_URL" -o "${TMPDIR}/${ASSET_NAME}.tar.gz"

echo "Extracting..."
tar -xzf "${TMPDIR}/${ASSET_NAME}.tar.gz" -C "$TMPDIR"

# Determine install location
if [ -w /usr/local/bin ]; then
  INSTALL_DIR="/usr/local/bin"
elif mkdir -p "$HOME/.local/bin" 2>/dev/null; then
  INSTALL_DIR="$HOME/.local/bin"
else
  echo "Cannot find a writable install location. Try running with sudo." >&2
  exit 1
fi

echo "Installing ${BINARY} to ${INSTALL_DIR}..."
mv "${TMPDIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
chmod +x "${INSTALL_DIR}/${BINARY}"

echo ""
echo "Cloesce ${LATEST_TAG} installed to ${INSTALL_DIR}/${BINARY}"

# PATH hint if install dir is not already on PATH
case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "NOTE: ${INSTALL_DIR} is not in your PATH."
    echo "Add the following to your shell profile (.bashrc, .zshrc, etc.):"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
