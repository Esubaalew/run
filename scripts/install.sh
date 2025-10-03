#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: install.sh [--version <tag>] [--prefix <dir>] [--repo <owner/name>] [--add-path]

Download a run release, extract it, and install the binary.

Options:
  --version <tag>   Install the specified tag (e.g. v0.1.0). Defaults to latest.
  --prefix <dir>    Destination directory for the binary (default: $HOME/.local/bin).
  --repo <name>     Override GitHub repository (default: Esubaalew/run).
  --add-path        Append the prefix to your shell profile if it's missing.
  -h, --help        Show this help message.
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required tool: $1" >&2
    exit 1
  fi
}

expand_path() {
  case "$1" in
    ~)
      printf '%s\n' "$HOME"
      ;;
    ~/*)
      printf '%s/%s\n' "$HOME" "${1:2}"
      ;;
    *)
      printf '%s\n' "$1"
      ;;
  esac
}

VERSION="latest"
PREFIX="${HOME}/.local/bin"
REPO="Esubaalew/run"
ADD_PATH=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    --prefix)
      PREFIX=$(expand_path "$2")
      shift 2
      ;;
    --repo)
      REPO="$2"
      shift 2
      ;;
    --add-path)
      ADD_PATH=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

require_cmd curl
require_cmd tar
require_cmd install

OS=$(uname -s)
ARCH=$(uname -m)

case "$ARCH" in
  x86_64|amd64)
    ARCH="x86_64"
    ;;
  arm64|aarch64)
    ARCH="aarch64"
    ;;
  *)
    echo "Unsupported architecture: $ARCH" >&2
    exit 1
    ;;
 esac

case "$OS" in
  Linux)
    TARGET="${ARCH}-unknown-linux-gnu"
    EXT="tar.gz"
    ;;
  Darwin)
    TARGET="${ARCH}-apple-darwin"
    EXT="tar.gz"
    if [[ "$ARCH" == "aarch64" ]]; then
      FALLBACK_TARGET="x86_64-apple-darwin"
    fi
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
 esac

if [[ "$VERSION" == "latest" ]]; then
  TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | head -n1 | cut -d '"' -f4)
else
  TAG="$VERSION"
fi

if [[ -z "$TAG" ]]; then
  echo "Failed to determine release tag" >&2
  exit 1
fi

ARCHIVE="run-${TARGET}-${TAG}.${EXT}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
FALLBACK_USED=false

if ! curl -fsI "$DOWNLOAD_URL" >/dev/null 2>&1; then
  if [[ -n "${FALLBACK_TARGET:-}" ]]; then
    echo "Native archive ${ARCHIVE} not found; falling back to ${FALLBACK_TARGET} (Rosetta required)." >&2
    TARGET="$FALLBACK_TARGET"
    ARCHIVE="run-${TARGET}-${TAG}.${EXT}"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
    FALLBACK_USED=true
    if ! curl -fsI "$DOWNLOAD_URL" >/dev/null 2>&1; then
      echo "Fallback archive also missing: ${DOWNLOAD_URL}" >&2
      exit 1
    fi
  else
    echo "Release asset not found: ${DOWNLOAD_URL}" >&2
    exit 1
  fi
fi

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

ARCHIVE_PATH="$TMPDIR/${ARCHIVE}"

echo "Downloading ${DOWNLOAD_URL}"
curl -fsSL "$DOWNLOAD_URL" -o "$ARCHIVE_PATH"

echo "Extracting ${ARCHIVE_PATH}"
mkdir -p "$TMPDIR/unpack"
tar -xzf "$ARCHIVE_PATH" -C "$TMPDIR/unpack"

INNER_DIR=$(find "$TMPDIR/unpack" -maxdepth 1 -type d -name "run-*" -print -quit)
if [[ -z "$INNER_DIR" ]]; then
  echo "Failed to locate extracted directory" >&2
  exit 1
fi

mkdir -p "$PREFIX"
install -m 0755 "$INNER_DIR/run" "$PREFIX/run"

echo "Installed run to $PREFIX/run"

if [[ ":$PATH:" != *":$PREFIX:"* ]]; then
  if [[ "$ADD_PATH" == true ]]; then
    SHELL_NAME=$(basename "${SHELL:-}")
    case "$SHELL_NAME" in
      zsh)
        CONFIG_FILE="${ZDOTDIR:-$HOME}/.zshrc"
        ;;
      bash)
        CONFIG_FILE="$HOME/.bashrc"
        ;;
      fish)
        CONFIG_FILE="$HOME/.config/fish/config.fish"
        ;;
      *)
        CONFIG_FILE="$HOME/.profile"
        ;;
    esac
    LINE="export PATH=\"$PREFIX:\$PATH\""
    if [[ "$SHELL_NAME" == "fish" ]]; then
      LINE="set -gx PATH $PREFIX \$PATH"
    fi
    if [[ -f "$CONFIG_FILE" ]]; then
      if ! grep -F "$LINE" "$CONFIG_FILE" >/dev/null 2>&1; then
        printf '\n%s\n' "$LINE" >> "$CONFIG_FILE"
        echo "Appended PATH update to $CONFIG_FILE"
      else
        echo "PATH entry already present in $CONFIG_FILE"
      fi
    else
      mkdir -p "$(dirname "$CONFIG_FILE")"
      printf '%s\n' "$LINE" > "$CONFIG_FILE"
      echo "Created $CONFIG_FILE with PATH update"
    fi
    echo "Reload your shell or run: source $CONFIG_FILE"
  else
    echo "run isn't currently on your PATH. Add the following line to your shell profile:"
    echo "    export PATH=\"$PREFIX:\$PATH\""
  fi
else
  echo "run is already on your PATH"
fi

if [[ "$FALLBACK_USED" == true ]]; then
  echo "Note: Installed x86_64 binary via Rosetta fallback. Install Rosetta 2 if you haven't already: softwareupdate --install-rosetta"
fi
