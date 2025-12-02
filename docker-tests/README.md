# Docker Tests for pg0

Automated tests to verify pg0 works correctly across different platforms and distributions.

## Test Matrix

| Test | Image | Platform | Architecture | libc |
|------|-------|----------|--------------|------|
| `test_debian_amd64.sh` | python:3.11-slim | linux/amd64 | x86_64 | glibc |
| `test_debian_arm64.sh` | python:3.11-slim | linux/arm64 | aarch64 | glibc |
| `test_alpine_amd64.sh` | python:3.11-alpine | linux/amd64 | x86_64 | musl |
| `test_alpine_arm64.sh` | python:3.11-alpine | linux/arm64 | aarch64 | musl |

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

### Using GitHub Token (Avoid Rate Limiting)

To avoid GitHub API rate limiting, set a personal access token:

```bash
# Create a token at https://github.com/settings/tokens
# No scopes needed for public repos

export GITHUB_TOKEN=ghp_your_token_here

# Now run tests with 5000 req/hour instead of 60
./test_debian_amd64.sh
```

The token is automatically passed into the Docker containers.

## Expected Results

### Debian/Ubuntu (glibc)
- ✅ PostgreSQL: Works
- ✅ pgvector: Works
- ✅ All queries: Success

### Alpine (musl)
- ✅ PostgreSQL: Works
- ⚠️ pgvector: May fail (no musl binaries available yet)
- ✅ Basic queries: Success

## Requirements

- Docker installed and running
- Internet connection (to download pg0 and PostgreSQL)
- ~500MB free space for PostgreSQL binaries

## Troubleshooting

### Test hangs during PostgreSQL download

This may be due to GitHub API rate limiting. Wait a few minutes and try again.

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
