#!/bin/bash
set -e

echo "=================================="
echo "Testing pg0 on Alpine AMD64"
echo "Image: python:3.12-alpine3.20"
echo "Platform: linux/amd64"
echo "=================================="

# Get the script directory to find install.sh
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
INSTALL_SCRIPT="$SCRIPT_DIR/../install.sh"

# Check if PG0_BINARY_PATH is set (local musl binary to test)
VOLUME_ARGS=""
BINARY_ENV=""
if [ -n "${PG0_BINARY_PATH:-}" ]; then
    echo "Using local binary: $PG0_BINARY_PATH"
    VOLUME_ARGS="-v $PG0_BINARY_PATH:/tmp/pg0-binary:ro"
    BINARY_ENV="-e PG0_BINARY_URL=file:///tmp/pg0-binary"
fi

# Note: Using Alpine 3.20 because the musl PostgreSQL binary requires ICU 74
# Alpine 3.22 has ICU 76 which is not compatible
docker run --rm --platform=linux/amd64 \
  $BINARY_ENV \
  -v "$INSTALL_SCRIPT:/tmp/install.sh:ro" \
  $VOLUME_ARGS \
  python:3.12-alpine3.20 sh -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing dependencies ==="
apk add --no-cache curl bash sudo procps shadow icu-libs lz4-libs libxml2 zstd-libs > /dev/null 2>&1

echo ""
echo "=== Creating non-root user ==="
adduser -D -s /bin/bash pguser
echo "pguser ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

echo ""
echo "=== Copying local install.sh ==="
cp /tmp/install.sh /usr/local/bin/install.sh
chmod 755 /usr/local/bin/install.sh

echo ""
echo "=== Switching to non-root user for pg0 ==="
su - pguser << EOF
set -e
export PG0_BINARY_URL="${PG0_BINARY_URL}"

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
pg0 info

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
pg0 psql -v ON_ERROR_STOP=1 -c "CREATE EXTENSION IF NOT EXISTS vector;"
pg0 psql -v ON_ERROR_STOP=1 -c "CREATE TABLE embeddings (id INT, vec vector(3));"
pg0 psql -v ON_ERROR_STOP=1 -c "INSERT INTO embeddings VALUES (1, '\''[1,2,3]'\''), (2, '\''[4,5,6]'\'');"
# Distance operators run the SIMD code paths that crashed with -march=native builds (#54)
pg0 psql -v ON_ERROR_STOP=1 -t -c "SELECT id, vec <-> '\''[1,1,1]'\'', vec <=> '\''[1,1,1]'\'' FROM embeddings ORDER BY 2;"
echo "✅ pgvector working correctly"

echo ""
echo "=== Stopping PostgreSQL ==="
pg0 stop

echo ""
echo "=================================="
echo "✅ ALL TESTS PASSED - Alpine AMD64"
echo "=================================="
EOF
'

echo ""
echo "✅ Test completed successfully!"
