#!/bin/bash
set -e

# This script downloads PostgreSQL and pgvector, bundles them together,
# and creates archives for distribution.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Load versions from versions.env
source "$SCRIPT_DIR/../versions.env"

PG_VERSION="${PG_VERSION}"
PGVECTOR_VERSION="${PGVECTOR_VERSION}"
PGVECTOR_COMPILED_TAG="${PGVECTOR_COMPILED_TAG}"
PGVECTOR_COMPILED_REPO="${PGVECTOR_COMPILED_REPO:-portalcorp/pgvector_compiled}"

# Platforms to build for (only those with pgvector pre-compiled binaries available)
PLATFORMS=(
    "aarch64-apple-darwin"
    "x86_64-unknown-linux-gnu"
    "x86_64-pc-windows-msvc"
)

BUILD_DIR="${SCRIPT_DIR}/../target/bundles"
OUTPUT_DIR="${SCRIPT_DIR}/../target/releases"

mkdir -p "$BUILD_DIR" "$OUTPUT_DIR"

echo "Building PostgreSQL $PG_VERSION + pgvector $PGVECTOR_VERSION bundles"
echo ""

for PLATFORM in "${PLATFORMS[@]}"; do
    echo "=== Building for $PLATFORM ==="

    WORK_DIR="$BUILD_DIR/$PLATFORM"
    rm -rf "$WORK_DIR"
    mkdir -p "$WORK_DIR"

    # Download PostgreSQL from theseus-rs
    PG_URL="https://github.com/theseus-rs/postgresql-binaries/releases/download/${PG_VERSION}/postgresql-${PG_VERSION}-${PLATFORM}.tar.gz"
    echo "Downloading PostgreSQL from $PG_URL"

    if ! curl -fsSL "$PG_URL" -o "$WORK_DIR/postgresql.tar.gz"; then
        echo "Warning: Failed to download PostgreSQL for $PLATFORM, skipping..."
        continue
    fi

    # Extract PostgreSQL
    echo "Extracting PostgreSQL..."
    tar -xzf "$WORK_DIR/postgresql.tar.gz" -C "$WORK_DIR"
    rm "$WORK_DIR/postgresql.tar.gz"

    # Find the extracted directory
    PG_DIR=$(find "$WORK_DIR" -maxdepth 1 -type d -name "postgresql-*" | head -1)
    if [ -z "$PG_DIR" ]; then
        # Try without version prefix
        PG_DIR="$WORK_DIR"
    fi

    # Download pgvector (pre-compiled)
    PG_MAJOR=$(echo "$PG_VERSION" | cut -d. -f1)
    PGVECTOR_URL="https://github.com/${PGVECTOR_COMPILED_REPO}/releases/download/${PGVECTOR_COMPILED_TAG}/pgvector-${PLATFORM}-pg${PG_MAJOR}.tar.gz"

    echo "Downloading pgvector from $PGVECTOR_URL"
    if curl -fsSL "$PGVECTOR_URL" -o "$WORK_DIR/pgvector.tar.gz"; then
        echo "Extracting pgvector..."
        mkdir -p "$WORK_DIR/pgvector"
        tar -xzf "$WORK_DIR/pgvector.tar.gz" -C "$WORK_DIR/pgvector"
        rm "$WORK_DIR/pgvector.tar.gz"

        # Copy pgvector files to PostgreSQL installation
        # pgvector installs: lib/vector.so (or .dylib or .dll), share/extension/vector.*
        if [ -d "$WORK_DIR/pgvector/lib" ]; then
            cp -r "$WORK_DIR/pgvector/lib/"* "$PG_DIR/lib/" 2>/dev/null || true
        fi
        if [ -d "$WORK_DIR/pgvector/share" ]; then
            cp -r "$WORK_DIR/pgvector/share/"* "$PG_DIR/share/" 2>/dev/null || true
        fi
        # Sometimes it's in a subdirectory
        find "$WORK_DIR/pgvector" \( -name "*.so" -o -name "*.dylib" -o -name "*.dll" \) | while read f; do
            cp "$f" "$PG_DIR/lib/" 2>/dev/null || true
        done
        find "$WORK_DIR/pgvector" -name "vector.control" | while read f; do
            cp "$f" "$PG_DIR/share/extension/" 2>/dev/null || true
        done
        find "$WORK_DIR/pgvector" -name "vector--*.sql" | while read f; do
            cp "$f" "$PG_DIR/share/extension/" 2>/dev/null || true
        done

        rm -rf "$WORK_DIR/pgvector"
        echo "pgvector installed successfully"
    else
        echo "ERROR: Could not download pgvector for $PLATFORM"
        exit 1
    fi

    # Create the bundle archive
    BUNDLE_NAME="postgresql-pgvector-${PG_VERSION}-${PLATFORM}.tar.gz"
    echo "Creating bundle: $BUNDLE_NAME"

    cd "$WORK_DIR"
    tar -czf "$OUTPUT_DIR/$BUNDLE_NAME" .
    cd - > /dev/null

    # Cleanup
    rm -rf "$WORK_DIR"

    echo "Created: $OUTPUT_DIR/$BUNDLE_NAME"
    echo ""
done

echo "=== Build complete ==="
ls -la "$OUTPUT_DIR"
