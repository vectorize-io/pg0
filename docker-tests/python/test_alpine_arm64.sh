#!/bin/bash
set -e

echo "=================================="
echo "Testing pg0 Python SDK on Alpine ARM64"
echo "Image: python:3.12-alpine3.20"
echo "Platform: linux/arm64"
echo "=================================="

# Get the script directory to find the SDK
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
SDK_DIR="$SCRIPT_DIR/../../sdk/python"

# Note: Using Alpine 3.20 because the musl PostgreSQL binary requires ICU 74
# Alpine 3.22 has ICU 76 which is not compatible
docker run --rm --platform=linux/arm64 \
  -v "$SDK_DIR:/sdk-src:ro" \
  python:3.12-alpine3.20 sh -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing system dependencies ==="
# procps is needed for pg0 to check if postgres process is running
# zstd-libs is needed for PostgreSQL compression support
apk add --no-cache bash sudo shadow icu-libs lz4-libs libxml2 procps zstd-libs > /dev/null 2>&1

echo ""
echo "=== Creating non-root user ==="
adduser -D -s /bin/bash pguser

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
export PATH="/usr/local/bin:\$PATH"

echo "=== Installing Python SDK (will download correct binary) ==="
cd /home/pguser/sdk
python3 -m pip install --user . -q

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

# Test pgvector (expected to fail on Alpine/musl)
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
    print("⚠️ pgvector extension failed (known limitation on Alpine/musl)")

# Stop PostgreSQL
print("")
print("=== Stopping PostgreSQL ===")
pg.stop()
pg.drop()

print("")
print("==================================")
print("✅ ALL TESTS PASSED - Alpine ARM64 Python SDK")
print("==================================")
PYEOF
EOF
'

echo ""
echo "✅ Test completed successfully!"
