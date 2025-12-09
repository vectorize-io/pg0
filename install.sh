#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

REPO="vectorize-io/pg0"
BINARY_NAME="pg0"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
detect_platform() {
    local os arch libc platform

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

    # Detect libc for Linux (musl vs glibc)
    if [ "$os" = "linux" ]; then
        if [ -f "/lib/ld-musl-${arch}.so.1" ] || [ -f "/lib/ld-musl-x86_64.so.1" ] || [ -f "/lib/ld-musl-aarch64.so.1" ]; then
            # Running on Alpine/musl
            libc="musl"
        else
            # Running on Debian/Ubuntu/etc with glibc
            libc="gnu"

            # Check glibc version - if too old, fall back to musl (statically linked)
            # Our gnu binaries require GLIBC 2.35+ (built on Ubuntu 22.04)
            if command -v ldd >/dev/null 2>&1; then
                glibc_version=$(ldd --version 2>&1 | head -n1 | grep -oE '[0-9]+\.[0-9]+' | head -n1)
                if [ -n "$glibc_version" ]; then
                    # Compare versions (2.35 minimum)
                    glibc_major=$(echo "$glibc_version" | cut -d. -f1)
                    glibc_minor=$(echo "$glibc_version" | cut -d. -f2)

                    if [ "$glibc_major" -lt 2 ] || ([ "$glibc_major" -eq 2 ] && [ "$glibc_minor" -lt 35 ]); then
                        echo -e "${YELLOW}Note: Detected old glibc ${glibc_version}. Using statically-linked musl binary for compatibility.${NC}"
                        libc="musl"
                    fi
                fi
            fi
        fi
        platform="${os}-${arch}-${libc}"
    else
        platform="${os}-${arch}"
    fi

    # Validate supported platforms
    case "$platform" in
        darwin-aarch64|linux-x86_64-gnu|linux-x86_64-musl|linux-aarch64-gnu|linux-aarch64-musl|windows-x86_64)
            ;;
        darwin-x86_64)
            echo -e "${YELLOW}Note: Intel Mac users can run the Apple Silicon binary via Rosetta 2${NC}"
            platform="darwin-aarch64"
            ;;
        *)
            echo -e "${RED}Unsupported platform: ${platform}${NC}"
            echo "Supported platforms:"
            echo "  - darwin-aarch64 (macOS Apple Silicon)"
            echo "  - linux-x86_64-gnu (Debian/Ubuntu x86_64)"
            echo "  - linux-x86_64-musl (Alpine x86_64)"
            echo "  - linux-aarch64-gnu (Debian/Ubuntu ARM64)"
            echo "  - linux-aarch64-musl (Alpine ARM64)"
            echo "  - windows-x86_64"
            exit 1
            ;;
    esac

    echo "$platform"
}

# Get latest release tag
get_latest_version() {
    local auth_header=""
    if [ -n "${GITHUB_TOKEN:-}" ]; then
        auth_header="Authorization: Bearer ${GITHUB_TOKEN}"
    fi

    if [ -n "$auth_header" ]; then
        curl -sL -H "$auth_header" "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    else
        curl -sL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/'
    fi
}

main() {
    echo -e "${GREEN}Installing pg0 - embedded PostgreSQL CLI...${NC}"

    local platform
    platform=$(detect_platform)
    echo "Detected platform: ${platform}"

    local ext=""
    if [[ "$platform" == windows* ]]; then
        ext=".exe"
    fi

    local url
    # Check if PG0_BINARY_URL is set (supports file:// and http(s)://)
    if [ -n "${PG0_BINARY_URL:-}" ]; then
        url="${PG0_BINARY_URL}"
        echo "Using custom binary URL: ${url}"
    else
        local version
        version=$(get_latest_version)
        if [ -z "$version" ]; then
            echo -e "${RED}Failed to fetch latest version${NC}"
            exit 1
        fi
        echo "Latest version: ${version}"

        local filename="${BINARY_NAME}-${platform}${ext}"
        url="https://github.com/${REPO}/releases/download/${version}/${filename}"
        echo "Downloading from: ${url}"
    fi

    # Create install directory if it doesn't exist
    mkdir -p "${INSTALL_DIR}"

    # Download/copy binary
    local tmp_file
    tmp_file=$(mktemp)

    if [[ "$url" == file://* ]]; then
        # Handle file:// URLs - just copy the file
        local file_path="${url#file://}"
        if [ ! -f "$file_path" ]; then
            echo -e "${RED}File not found: ${file_path}${NC}"
            rm -f "${tmp_file}"
            exit 1
        fi
        cp "$file_path" "${tmp_file}"
    else
        # Handle http(s):// URLs
        if ! curl -fsSL "${url}" -o "${tmp_file}"; then
            echo -e "${RED}Failed to download binary${NC}"
            rm -f "${tmp_file}"
            exit 1
        fi
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
    echo "pg0 is now available."
}

main "$@"
