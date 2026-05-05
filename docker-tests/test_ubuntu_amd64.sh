#!/bin/bash
# Test pg0 against the official Ubuntu LTS / interim images.
# 24.04 (Noble) is currently the only supported LTS; 25.10 (Plucky) and the
# upcoming 26.04 ship libxml2 2.14 (SONAME .so.16) which breaks the bundled
# theseus-rs PostgreSQL binary that links against libxml2.so.2.
set -u

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
INSTALL_SCRIPT="$SCRIPT_DIR/../install.sh"

VOLUME_ARGS=""
BINARY_ENV=""
if [ -n "${PG0_BINARY_PATH:-}" ]; then
    echo "Using local binary: $PG0_BINARY_PATH"
    VOLUME_ARGS="-v $PG0_BINARY_PATH:/tmp/pg0-binary:ro"
    BINARY_ENV="-e PG0_BINARY_URL=file:///tmp/pg0-binary"
elif [ -n "${PG0_VERSION:-}" ]; then
    echo "Using released binary v$PG0_VERSION"
    BINARY_ENV="-e PG0_BINARY_URL=https://github.com/vectorize-io/pg0/releases/download/v${PG0_VERSION}/pg0-linux-x86_64-gnu"
fi

run_one() {
    local image="$1"
    echo ""
    echo "=================================="
    echo "Testing pg0 on $image (linux/amd64)"
    echo "=================================="

    docker run --rm --platform=linux/amd64 \
      $BINARY_ENV \
      -v "$INSTALL_SCRIPT:/tmp/install.sh:ro" \
      $VOLUME_ARGS \
      -e DEBIAN_FRONTEND=noninteractive \
      "$image" bash -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing dependencies ==="
# Some hosts cannot reach the http:// Ubuntu mirrors; switch to https.
if ls /etc/apt/sources.list.d/*.sources >/dev/null 2>&1; then
    sed -i "s|http://archive.ubuntu.com|https://archive.ubuntu.com|g; s|http://security.ubuntu.com|https://security.ubuntu.com|g" /etc/apt/sources.list.d/*.sources
fi
echo "Acquire::https::Verify-Peer false;" > /etc/apt/apt.conf.d/99insecure
echo "Acquire::https::Verify-Host false;" >> /etc/apt/apt.conf.d/99insecure
apt-get update -qq
# README-recommended runtime deps + tzdata + libreadline (for psql).
# Some package names differ across releases - fall back across them.
apt-get install -y -qq curl ca-certificates sudo procps tzdata >/dev/null
apt-get install -y -qq libgssapi-krb5-2 >/dev/null
# libssl3 is a virtual that resolves to libssl3t64 on 24.04+
apt-get install -y -qq libssl3 >/dev/null 2>&1 || apt-get install -y -qq libssl3t64 >/dev/null
# libxml2 was renamed to libxml2-16 in 25.10 (SONAME .so.2 -> .so.16)
apt-get install -y -qq libxml2 >/dev/null 2>&1 || apt-get install -y -qq libxml2-16 >/dev/null
# libicu major version varies by release
apt-get install -y -qq libicu74 >/dev/null 2>&1 || \
    apt-get install -y -qq libicu76 >/dev/null 2>&1 || \
    apt-get install -y -qq libicu72 >/dev/null
# readline for psql
apt-get install -y -qq libreadline8 >/dev/null 2>&1 || \
    apt-get install -y -qq libreadline8t64 >/dev/null

echo ""
echo "=== Creating non-root user ==="
useradd -m -s /bin/bash pguser

echo ""
echo "=== Switching to non-root user for pg0 ==="
su - pguser << EOF
set -e
export PG0_BINARY_URL="${PG0_BINARY_URL:-}"

echo "=== Installing pg0 ==="
bash /tmp/install.sh
export PATH="\$HOME/.local/bin:\$PATH"

echo ""
echo "=== Starting PostgreSQL ==="
pg0 start
sleep 3

echo ""
echo "=== Basic query ==="
pg0 psql -c "SELECT version();" -t | head -1

echo ""
echo "=== pgvector ==="
pg0 psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
pg0 psql -c "CREATE TABLE embeddings (id INT, vec vector(3));"
pg0 psql -c "INSERT INTO embeddings VALUES (1, '\''[1,2,3]'\'');"
pg0 psql -c "SELECT * FROM embeddings;" -t

echo ""
echo "=== Stopping PostgreSQL ==="
pg0 stop

echo ""
echo "PASS: $image"
EOF
'
    local rc=$?
    if [ $rc -ne 0 ]; then
        echo "FAIL: $image (exit $rc)"
    fi
    return $rc
}

failures=()
for image in ubuntu:24.04 ubuntu:25.10; do
    if ! run_one "$image"; then
        failures+=("$image")
    fi
done

echo ""
echo "=================================="
if [ ${#failures[@]} -eq 0 ]; then
    echo "All Ubuntu tests passed"
    exit 0
else
    echo "Failures: ${failures[*]}"
    exit 1
fi
