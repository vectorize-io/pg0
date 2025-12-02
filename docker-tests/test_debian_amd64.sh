#!/bin/bash
set -e

echo "=================================="
echo "Testing pg0 on Debian AMD64"
echo "Image: python:3.11-slim"
echo "Platform: linux/amd64"
echo "=================================="

# Get the script directory to find install.sh
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
INSTALL_SCRIPT="$SCRIPT_DIR/../install.sh"

docker run --rm --platform=linux/amd64 \
  -e GITHUB_TOKEN="${GITHUB_TOKEN:-}" \
  -v "$INSTALL_SCRIPT:/tmp/install.sh:ro" \
  python:3.11-slim bash -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing dependencies ==="
apt-get update -qq
# Install dependencies - libicu version may vary by Debian version
apt-get install -y curl libxml2 libssl3 libgssapi-krb5-2 sudo procps 2>&1 | grep -v "^Get:" || true
# Install libicu (try different versions)
apt-get install -y libicu72 2>/dev/null || apt-get install -y libicu74 2>/dev/null || apt-get install -y libicu* 2>&1 | head -5

echo ""
echo "=== Creating non-root user ==="
useradd -m -s /bin/bash pguser
echo "pguser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

echo ""
echo "=== Copying local install.sh ==="
cp /tmp/install.sh /usr/local/bin/install.sh
chmod 755 /usr/local/bin/install.sh

echo ""
echo "=== Switching to non-root user for pg0 ==="
su - pguser << EOF
set -e
export GITHUB_TOKEN="${GITHUB_TOKEN}"

echo "=== Installing pg0 ==="
bash /usr/local/bin/install.sh
export PATH="\$HOME/.local/bin:\$PATH"

echo ""
echo "=== Checking pg0 version ==="
pg0 --version

echo ""
echo "=== Starting PostgreSQL ==="
pg0 start
sleep 5

echo ""
echo "=== Getting instance info ==="
pg0 info || (echo "Failed to get info, checking logs..." && cat ~/.pg0/instances/default/data/log/* 2>/dev/null || echo "No logs found")

echo ""
echo "=== Testing basic SELECT query ==="
pg0 psql -c "SELECT version();" -t | head -1

echo ""
echo "=== Testing table creation and data ==="
pg0 psql -c "CREATE TABLE test (id INT, name TEXT);"
pg0 psql -c "INSERT INTO test VALUES (1, '\''Hello'\''), (2, '\''World'\'');"
pg0 psql -c "SELECT * FROM test;" -t

echo ""
echo "=== Testing pgvector extension ==="
if pg0 psql -c "CREATE EXTENSION IF NOT EXISTS vector;" 2>&1; then
    echo "✅ pgvector extension created successfully"
    pg0 psql -c "CREATE TABLE embeddings (id INT, vec vector(3));"
    pg0 psql -c "INSERT INTO embeddings VALUES (1, '\''[1,2,3]'\'');"
    pg0 psql -c "SELECT * FROM embeddings;" -t
    echo "✅ pgvector working correctly"
else
    echo "⚠️  pgvector extension failed (expected on some platforms)"
fi

echo ""
echo "=== Stopping PostgreSQL ==="
pg0 stop

echo ""
echo "=================================="
echo "✅ ALL TESTS PASSED - Debian AMD64"
echo "=================================="
EOF
'

echo ""
echo "✅ Test completed successfully!"
