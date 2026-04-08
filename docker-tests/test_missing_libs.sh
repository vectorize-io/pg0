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

# PG0_BINARY_PATH is required - this test must use a binary built from source
# (with the shared library detection code), not the released binary
if [ -z "${PG0_BINARY_PATH:-}" ]; then
    echo "ERROR: PG0_BINARY_PATH must be set to a Linux binary built from this branch"
    exit 1
fi

echo "Using local binary: $PG0_BINARY_PATH"

# Create a temporary test script to run inside the container
# This avoids nested heredoc quoting issues
TEMP_SCRIPT=$(mktemp)
cat > "$TEMP_SCRIPT" << 'INNERSCRIPT'
#!/bin/bash
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
echo "=== Installing pg0 from local binary ==="
cp /tmp/pg0-binary /usr/local/bin/pg0
chmod 755 /usr/local/bin/pg0

echo ""
echo "=== Phase 1: Initial extraction with all deps present ==="
su -s /bin/bash - pguser -c '
set -e
export PATH="/usr/local/bin:$PATH"

echo "=== Starting PostgreSQL (initial extraction) ==="
pg0 start
sleep 3

echo "=== Stopping PostgreSQL ==="
pg0 stop
sleep 1

echo "=== Removing extracted installation to force re-extraction ==="
rm -rf ~/.pg0/installation
echo "Installation directory cleared."
'

echo ""
echo "=== Phase 2: Remove libxml2 to simulate missing library ==="
apt-get remove -y libxml2 2>&1 | tail -3

echo ""
echo "=== Phase 3: Verify pg0 detects missing libraries ==="
su -s /bin/bash - pguser -c '
set -e
export PATH="/usr/local/bin:$PATH"

echo "=== Starting pg0 (should fail with missing library error) ==="
OUTPUT=$(pg0 start 2>&1 || true)
echo "$OUTPUT"

echo ""
echo "=== Checking error message ==="

if echo "$OUTPUT" | grep -q "missing required system libraries"; then
    echo "PASS: Found missing required system libraries message"
else
    echo "FAIL: Missing expected error message about shared libraries"
    exit 1
fi

if echo "$OUTPUT" | grep -qi "libxml2"; then
    echo "PASS: Found libxml2 in the missing library list"
else
    echo "FAIL: Expected libxml2 to be listed as missing"
    exit 1
fi

if echo "$OUTPUT" | grep -q "Install the missing libraries"; then
    echo "PASS: Found install guidance message"
else
    echo "FAIL: Missing install guidance"
    exit 1
fi

echo ""
echo "============================================="
echo "ALL CHECKS PASSED - Missing libs detected"
echo "============================================="
'
INNERSCRIPT

docker run --rm --platform=linux/amd64 \
  -v "$PG0_BINARY_PATH:/tmp/pg0-binary:ro" \
  -v "$TEMP_SCRIPT:/tmp/test_script.sh:ro" \
  python:3.11-slim bash /tmp/test_script.sh

rm -f "$TEMP_SCRIPT"

echo ""
echo "Test completed successfully!"
