# Docker Tests for pg0

Automated tests to verify pg0 works correctly across different platforms and distributions.

## CLI Tests

| Test | Image | Platform | Architecture | libc |
|------|-------|----------|--------------|------|
| `test_debian_amd64.sh` | python:3.11-slim | linux/amd64 | x86_64 | glibc |
| `test_debian_arm64.sh` | python:3.11-slim | linux/arm64 | aarch64 | glibc |
| `test_alpine_amd64.sh` | python:3.12-alpine3.20 | linux/amd64 | x86_64 | musl |
| `test_alpine_arm64.sh` | python:3.12-alpine3.20 | linux/arm64 | aarch64 | musl |

## Python SDK Tests

| Test | Image | Platform | Architecture | libc |
|------|-------|----------|--------------|------|
| `python/test_debian_amd64.sh` | python:3.11-slim | linux/amd64 | x86_64 | glibc |
| `python/test_debian_arm64.sh` | python:3.11-slim | linux/arm64 | aarch64 | glibc |
| `python/test_alpine_amd64.sh` | python:3.12-alpine3.20 | linux/amd64 | x86_64 | musl |
| `python/test_alpine_arm64.sh` | python:3.12-alpine3.20 | linux/arm64 | aarch64 | musl |

The Python SDK tests install the package via `pip install .` and verify the bundled binary works correctly.

## What Each Test Does

1. **System Check** - Verifies architecture and OS
2. **Install pg0** - Downloads and installs pg0 CLI
3. **Start PostgreSQL** - Starts embedded PostgreSQL server
4. **Basic SELECT** - Tests PostgreSQL connectivity
5. **Table Operations** - Creates table, inserts data, queries
6. **pgvector Test** - Tests vector extension (if available)
7. **Cleanup** - Stops PostgreSQL

## Running Tests

### Run All Tests

```bash
cd docker-tests
chmod +x *.sh
./run_all_tests.sh
```

### Run Individual Tests

```bash
# Test Debian AMD64 (most common)
./test_debian_amd64.sh

# Test Debian ARM64 (M1/M2/M3 Macs, AWS Graviton)
./test_debian_arm64.sh

# Test Alpine AMD64
./test_alpine_amd64.sh

# Test Alpine ARM64
./test_alpine_arm64.sh
```

## Expected Results

### Debian/Ubuntu (glibc)
- ✅ PostgreSQL: Works
- ✅ pgvector: Works
- ✅ All queries: Success

### Alpine (musl)
- ✅ PostgreSQL: Works (requires `icu-libs`, `lz4-libs`, `libxml2`, `zstd-libs`, `procps` packages and ICU 74 - use Alpine 3.20)
- ⚠️ pgvector: Fails (no musl binaries available - glibc-only)
- ✅ Basic queries: Success

**Note:** The PostgreSQL musl binary is built against ICU 74. Alpine 3.22+ uses ICU 76 which is not compatible. Use Alpine 3.20 for musl-based deployments.

## Requirements

- Docker installed and running
- Internet connection (to download pg0 binary)
- ~50MB free space (PostgreSQL and pgvector are bundled in the binary)

## Troubleshooting

### ARM64 tests are slow

ARM64 tests use emulation on x86_64 hosts, which is slower. This is expected.

### pgvector fails on Alpine

This is a known limitation. pgvector binaries are currently only available for glibc, not musl.

## CI Integration

These tests can be integrated into GitHub Actions:

```yaml
- name: Test pg0 on Debian AMD64
  run: |
    cd docker-tests
    ./test_debian_amd64.sh
```

## Adding New Tests

To add a new platform test:

1. Copy an existing test script
2. Modify the image and platform
3. Update the Docker command as needed
4. Add it to `run_all_tests.sh`
