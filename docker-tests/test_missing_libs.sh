#!/bin/bash
set -e

echo "============================================="
echo "Testing pg0 missing shared library detection"
echo "Image: python:3.11-slim"
echo "Platform: linux/amd64"
echo "============================================="

# Get the script directory to find install.sh
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
INSTALL_SCRIPT="$SCRIPT_DIR/../install.sh"

# Check if PG0_BINARY_PATH is set (local binary to test)
VOLUME_ARGS=""
BINARY_ENV=""
if [ -n "${PG0_BINARY_PATH:-}" ]; then
    echo "Using local binary: $PG0_BINARY_PATH"
    VOLUME_ARGS="-v $PG0_BINARY_PATH:/tmp/pg0-binary:ro"
    BINARY_ENV="-e PG0_BINARY_URL=file:///tmp/pg0-binary"
fi

docker run --rm --platform=linux/amd64 \
  $BINARY_ENV \
  -v "$INSTALL_SCRIPT:/tmp/install.sh:ro" \
  $VOLUME_ARGS \
  python:3.11-slim bash -c '
set -e

echo "=== System Info ==="
uname -m
cat /etc/os-release | grep PRETTY_NAME

echo ""
echo "=== Installing dependencies ==="
apt-get update -qq
apt-get install -y curl libxml2 libssl3 libgssapi-krb5-2 sudo procps 2>&1 | grep -v "^Get:" || true
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
echo "=== Phase 1: Install pg0 and do initial extraction ==="
su - pguser << EOF
set -e
export PG0_BINARY_URL="${PG0_BINARY_URL}"

echo "=== Installing pg0 ==="
bash /usr/local/bin/install.sh
export PATH="\$HOME/.local/bin:\$PATH"

echo ""
echo "=== Starting PostgreSQL (initial extraction) ==="
pg0 start
sleep 3

echo ""
echo "=== Stopping PostgreSQL ==="
pg0 stop
sleep 1

echo ""
echo "=== Removing extracted installation to force re-extraction ==="
rm -rf ~/.pg0/installation
echo "Installation directory cleared."
EOF

echo ""
echo "=== Phase 2: Remove libxml2 to simulate missing library ==="
apt-get remove -y libxml2 2>&1 | tail -3

echo ""
echo "=== Phase 3: Verify pg0 detects missing libraries ==="
su - pguser << EOF
set -e
export PATH="\$HOME/.local/bin:\$PATH"

echo "=== Starting pg0 (should fail with missing library error) ==="
OUTPUT=\$(pg0 start 2>&1 || true)
EXIT_CODE=\${PIPESTATUS[0]:-\$?}
echo "\$OUTPUT"

echo ""
echo "=== Checking error message ==="

if echo "\$OUTPUT" | grep -q "missing required system libraries"; then
    echo "PASS: Found 'missing required system libraries' message"
else
    echo "FAIL: Missing expected error message about shared libraries"
    exit 1
fi

if echo "\$OUTPUT" | grep -q "libxml2"; then
    echo "PASS: Found 'libxml2' in the missing library list"
else
    echo "FAIL: Expected libxml2 to be listed as missing"
    exit 1
fi

if echo "\$OUTPUT" | grep -q "Install the missing libraries"; then
    echo "PASS: Found install guidance message"
else
    echo "FAIL: Missing install guidance"
    exit 1
fi

echo ""
echo "============================================="
echo "ALL CHECKS PASSED - Missing libs detected"
echo "============================================="
EOF
'

echo ""
echo "Test completed successfully!"
