#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

REPO="vectorize-io/embedded-pg-cli"
BINARY_NAME="embedded-postgres"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
detect_platform() {
    local os arch

    case "$(uname -s)" in
        Linux*)     os="linux";;
        Darwin*)    os="darwin";;
        MINGW*|MSYS*|CYGWIN*)  os="windows";;
        *)          echo -e "${RED}Unsupported operating system: $(uname -s)${NC}"; exit 1;;
    esac

    case "$(uname -m)" in
        x86_64|amd64)   arch="x86_64";;
        arm64|aarch64)  arch="aarch64";;
        *)              echo -e "${RED}Unsupported architecture: $(uname -m)${NC}"; exit 1;;
    esac

    echo "${os}-${arch}"
}

# Get latest release tag
get_latest_version() {
    curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
}

main() {
    echo -e "${GREEN}Installing embedded-postgres CLI...${NC}"

    local platform
    platform=$(detect_platform)
    echo "Detected platform: ${platform}"

    local version
    version=$(get_latest_version)
    if [ -z "$version" ]; then
        echo -e "${RED}Failed to fetch latest version${NC}"
        exit 1
    fi
    echo "Latest version: ${version}"

    local ext=""
    if [[ "$platform" == windows* ]]; then
        ext=".exe"
    fi

    local filename="${BINARY_NAME}-${platform}${ext}"
    local url="https://github.com/${REPO}/releases/download/${version}/${filename}"

    echo "Downloading from: ${url}"

    # Create install directory if it doesn't exist
    mkdir -p "${INSTALL_DIR}"

    # Download binary
    local tmp_file
    tmp_file=$(mktemp)
    if ! curl -fsSL "${url}" -o "${tmp_file}"; then
        echo -e "${RED}Failed to download binary${NC}"
        rm -f "${tmp_file}"
        exit 1
    fi

    # Move to install directory
    mv "${tmp_file}" "${INSTALL_DIR}/${BINARY_NAME}${ext}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}${ext}"

    echo -e "${GREEN}Successfully installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}${ext}${NC}"

    # Check if install dir is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo -e "${YELLOW}NOTE: ${INSTALL_DIR} is not in your PATH.${NC}"
        echo "Add it to your shell configuration:"
        echo ""
        echo "  export PATH=\"\$PATH:${INSTALL_DIR}\""
        echo ""
    fi

    echo ""
    echo "Usage:"
    echo "  ${BINARY_NAME} start    # Start PostgreSQL"
    echo "  ${BINARY_NAME} stop     # Stop PostgreSQL"
    echo "  ${BINARY_NAME} status   # Check status"
    echo "  ${BINARY_NAME} uri      # Get connection URI"
}

main "$@"
