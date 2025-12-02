#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

FAILED_TESTS=()
PASSED_TESTS=()

run_test() {
    local test_name=$1
    local test_script=$2

    echo ""
    echo -e "${BLUE}======================================${NC}"
    echo -e "${BLUE}Running: $test_name${NC}"
    echo -e "${BLUE}======================================${NC}"

    if bash "$test_script"; then
        PASSED_TESTS+=("$test_name")
        echo -e "${GREEN}✅ $test_name PASSED${NC}"
    else
        FAILED_TESTS+=("$test_name")
        echo -e "${RED}❌ $test_name FAILED${NC}"
    fi
}

# Get the directory where this script is located
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}pg0 Docker Test Suite${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""
echo "This will test pg0 on multiple platforms:"
echo "  - Debian AMD64 (python:3.11-slim)"
echo "  - Debian ARM64 (python:3.11-slim)"
echo "  - Alpine AMD64 (python:3.11-alpine)"
echo "  - Alpine ARM64 (python:3.11-alpine)"
echo ""
echo -e "${YELLOW}Note: ARM64 tests will use emulation on x86_64 hosts${NC}"
echo ""
read -p "Press Enter to continue..."

# Run all tests
run_test "Debian AMD64" "$DIR/test_debian_amd64.sh"
run_test "Debian ARM64" "$DIR/test_debian_arm64.sh"
run_test "Alpine AMD64" "$DIR/test_alpine_amd64.sh"
run_test "Alpine ARM64" "$DIR/test_alpine_arm64.sh"

# Print summary
echo ""
echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Test Summary${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

if [ ${#PASSED_TESTS[@]} -gt 0 ]; then
    echo -e "${GREEN}Passed (${#PASSED_TESTS[@]}):${NC}"
    for test in "${PASSED_TESTS[@]}"; do
        echo -e "  ${GREEN}✅${NC} $test"
    done
fi

if [ ${#FAILED_TESTS[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}Failed (${#FAILED_TESTS[@]}):${NC}"
    for test in "${FAILED_TESTS[@]}"; do
        echo -e "  ${RED}❌${NC} $test"
    done
    echo ""
    exit 1
fi

echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}🎉 ALL TESTS PASSED!${NC}"
echo -e "${GREEN}========================================${NC}"
