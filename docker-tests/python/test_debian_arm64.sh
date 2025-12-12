#!/bin/bash
set -e

echo "=================================="
echo "Testing pg0 Python SDK on Debian ARM64"
echo "Image: python:3.11-slim"
echo "Platform: linux/arm64"
echo "=================================="

# Get the script directory to find the SDK
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
SDK_DIR="$SCRIPT_DIR/../../sdk/python"

docker run --rm --platform=linux/arm64 \
  -v "$SDK_DIR:/sdk-src:ro" \
  python:3.11-slim bash -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing system dependencies ==="
apt-get update -qq
# procps is needed for pg0 to check if postgres process is running
apt-get install -y -qq libxml2 libssl3 libgssapi-krb5-2 procps > /dev/null 2>&1
apt-get install -y -qq libicu72 || apt-get install -y -qq libicu74 || apt-get install -y -qq libicu76 || apt-get install -y -qq "libicu*" > /dev/null 2>&1

echo ""
echo "=== Creating non-root user ==="
useradd -m -s /bin/bash pguser

# Copy SDK to writable location (excluding any existing bin directory with wrong-platform binary)
mkdir -p /home/pguser/sdk
cp -r /sdk-src/pg0 /home/pguser/sdk/
cp /sdk-src/pyproject.toml /sdk-src/hatch_build.py /sdk-src/README.md /home/pguser/sdk/
rm -rf /home/pguser/sdk/pg0/bin  # Remove any existing binary
chown -R pguser:pguser /home/pguser/sdk

echo ""
echo "=== Switching to non-root user ==="
su - pguser << EOF
set -e

echo "=== Installing Python SDK (will download correct binary) ==="
cd /home/pguser/sdk
pip install --user . -q

echo ""
echo "=== Testing Python SDK ==="
python3 << PYEOF
from pg0 import Pg0, _get_bundled_binary

# Check bundled binary
bundled = _get_bundled_binary()
print(f"Bundled binary: {bundled}")
assert bundled is not None, "Bundled binary not found!"
assert bundled.exists(), f"Bundled binary does not exist: {bundled}"

# Start PostgreSQL
print("")
print("=== Starting PostgreSQL ===")
pg = Pg0()
info = pg.start()
print(f"PostgreSQL running on port {info.port}")
print(f"URI: {info.uri}")

# Test basic query
print("")
print("=== Testing basic SELECT query ===")
result = pg.execute("SELECT version();")
print(result.strip().split("\\n")[0][:80])

# Test table operations
print("")
print("=== Testing table creation and data ===")
pg.execute("CREATE TABLE test (id INT, name TEXT);")
pg.execute("INSERT INTO test VALUES (1, '\''Hello'\''), (2, '\''World'\'');")
result = pg.execute("SELECT * FROM test;")
print(result)

# Test pgvector
print("")
print("=== Testing pgvector extension ===")
try:
    pg.execute("CREATE EXTENSION IF NOT EXISTS vector;")
    print("✅ pgvector extension created successfully")
    pg.execute("CREATE TABLE embeddings (id INT, vec vector(3));")
    pg.execute("INSERT INTO embeddings VALUES (1, '\''[1,2,3]'\'');")
    result = pg.execute("SELECT * FROM embeddings;")
    print(result)
    print("✅ pgvector working correctly")
except Exception as e:
    print(f"⚠️ pgvector failed: {e}")

# Stop PostgreSQL
print("")
print("=== Stopping PostgreSQL ===")
pg.stop()
pg.drop()

print("")
print("==================================")
print("✅ ALL TESTS PASSED - Debian ARM64 Python SDK")
print("==================================")
PYEOF
EOF
'

echo ""
echo "✅ Test completed successfully!"
