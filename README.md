# pg0

[![PyPI version](https://badge.fury.io/py/pg0-embedded.svg)](https://pypi.org/project/pg0-embedded/)
[![PyPI downloads](https://img.shields.io/pypi/dm/pg0-embedded.svg)](https://pypi.org/project/pg0-embedded/)
[![Python versions](https://img.shields.io/pypi/pyversions/pg0-embedded.svg)](https://pypi.org/project/pg0-embedded/)

**Zero-config PostgreSQL with pgvector.**

A single binary that runs PostgreSQL locally - no installation, no configuration, no Docker required. Includes **pgvector** for AI/vector workloads out of the box.

## Why pg0?

PostgreSQL setup is painful. Docker adds complexity. Local installs conflict with system packages. pg0 gives you a real PostgreSQL server with zero friction:

- **No installation** - download a single binary and run `pg0 start`
- **No Docker** - no containers, no daemon, no complexity
- **No configuration** - sensible defaults, just works
- **Production parity** - develop with the same database you'll deploy
- **Full PostgreSQL** - JSON, arrays, CTEs, window functions, extensions, pgvector - everything works

Use pg0 for local development, testing, CI/CD pipelines, or any scenario where you want PostgreSQL without the setup overhead.

## Supported Platforms

This table describes which **binaries** we publish. Whether a binary actually runs on a given OS release depends on the libraries that distro ships - see [Tested and Supported Platforms](#tested-and-supported-platforms) for the per-distribution story (e.g. Alpine 3.20-3.21 work, Alpine 3.22+ does not).

| Platform | Architecture | Binary |
|----------|--------------|--------|
| macOS | Apple Silicon (M1/M2/M3) | `pg0-darwin-aarch64` |
| macOS | Intel | `pg0-darwin-x86_64` |
| Linux | x86_64 (glibc, e.g. Debian/Ubuntu) | `pg0-linux-x86_64-gnu` |
| Linux | x86_64 (musl, e.g. Alpine) | `pg0-linux-x86_64-musl` |
| Linux | ARM64 (glibc) | `pg0-linux-aarch64-gnu` |
| Linux | ARM64 (musl) | `pg0-linux-aarch64-musl` |
| Windows | x64 | `pg0-windows-x86_64.exe` |

## Features

- **Zero dependencies** - single binary, works offline
- **PostgreSQL 18** with pgvector 0.8.1 bundled
- **Multiple instances** - run multiple PostgreSQL servers simultaneously
- **Cross-platform** - macOS (Apple Silicon), Linux (x86_64 & ARM64), Windows (x64)
- **Language SDKs** - Python and Node.js libraries for programmatic control
- **Bundled psql** - no separate client installation needed
- **Persistent data** - survives restarts, stored in `~/.pg0/`

## Installation

### CLI Binary

The install script automatically detects your platform and downloads the correct binary:

```bash
curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
```

Or with a custom install directory:

```bash
INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
```

### Python SDK

Install via pip:

```bash
pip install pg0-embedded
```

Quick start:

```python
from pg0 import Pg0

# Start PostgreSQL
pg = Pg0()
pg.start()
print(pg.uri)  # postgresql://postgres:postgres@127.0.0.1:5432/postgres

# Or use context manager
with Pg0() as pg:
    result = pg.execute("SELECT version();")
    print(result)
```

See [PyPI package](https://pypi.org/project/pg0-embedded/) for more details.

### Node.js SDK

Install via npm:

```bash
npm install @vectorize-io/pg0
```

Quick start:

```typescript
import { Pg0 } from '@vectorize-io/pg0';

const pg = new Pg0();
await pg.start();
console.log(await pg.getUri());
await pg.stop();
```

### Linux Distributions

pg0 provides separate binaries optimized for different Linux distributions:

- **Debian/Ubuntu/RHEL** (glibc-based): Uses `pg0-linux-{arch}-gnu`
- **Alpine** (musl-based): Uses `pg0-linux-{arch}-musl`

The install script automatically detects your distribution and downloads the correct binary.

## Docker

pg0 works in Docker containers. Here are the minimal setup steps for each supported image type:

### Debian/Ubuntu (glibc-based)

```dockerfile
FROM debian:bookworm-slim
# or: python:3.11-slim, ubuntu:22.04, ubuntu:24.04, ubuntu:25.10, etc.

# Install required dependencies. libxml2 and ICU are bundled into the pg0
# binary so they do not need to be installed - this means pg0 works on
# Ubuntu 25.10+ where libxml2.so.2 has been replaced by libxml2.so.16.
# tzdata is needed because PostgreSQL reads /usr/share/zoneinfo at startup,
# and libreadline is needed by `pg0 psql`.
RUN apt-get update && apt-get install -y \
    curl \
    libssl3 \
    libgssapi-krb5-2 \
    tzdata \
    libreadline8 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (PostgreSQL cannot run as root)
RUN useradd -m -s /bin/bash pguser
USER pguser

# Install pg0
RUN curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
ENV PATH="/home/pguser/.local/bin:${PATH}"

# Start PostgreSQL when container runs
CMD ["bash", "-c", "pg0 start && tail -f /dev/null"]
```

Or start it with your application:

```bash
docker run -d myimage bash -c "pg0 start && exec your-application"
```

### Alpine (musl-based)

**Note:** The musl binary requires ICU 74. Use Alpine 3.20 (not 3.22+) as newer versions have ICU 76.

```dockerfile
FROM alpine:3.20
# or: python:3.12-alpine3.20

# Install required dependencies
RUN apk add --no-cache curl bash shadow icu-libs lz4-libs libxml2

# Create non-root user (PostgreSQL cannot run as root)
RUN adduser -D -s /bin/bash pguser
USER pguser

# Install pg0
RUN curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
ENV PATH="/home/pguser/.local/bin:${PATH}"

# Start PostgreSQL when container runs
CMD ["sh", "-c", "pg0 start && tail -f /dev/null"]
```

Or start it with your application:

```bash
docker run -d myimage sh -c "pg0 start && exec your-application"
```

### Quick Test

Run pg0 in a Docker container with a single command:

```bash
# Debian/Ubuntu (works on 22.04, 24.04, 25.10, 26.04, ...)
docker run --rm -it python:3.11-slim bash -c '
  apt-get update -qq &&
  apt-get install -y curl libssl3 libgssapi-krb5-2 tzdata libreadline8 &&
  useradd -m pguser &&
  su - pguser -c "curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash &&
    export PATH=\"\$HOME/.local/bin:\$PATH\" &&
    pg0 start &&
    sleep 3 &&
    pg0 psql -c \"SELECT version();\""
'

# Alpine (use 3.20 for ICU 74 compatibility)
docker run --rm -it python:3.12-alpine3.20 sh -c '
  apk add --no-cache curl bash shadow icu-libs lz4-libs libxml2 &&
  adduser -D pguser &&
  su - pguser -c "curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash &&
    export PATH=\"\$HOME/.local/bin:\$PATH\" &&
    pg0 start &&
    sleep 3 &&
    pg0 psql -c \"SELECT version();\""
'
```

**Note:** PostgreSQL requires a non-root user for security. The examples above create a `pguser` for this purpose.

## Quick Start

```bash
# Start PostgreSQL
pg0 start

# Connect with psql
pg0 psql

# Use pgvector
pg0 psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
pg0 psql -c "CREATE TABLE items (embedding vector(3));"
pg0 psql -c "INSERT INTO items VALUES ('[1,2,3]');"

# Stop when done
pg0 stop
```

## Usage

### Commands

pg0 provides the following commands:

1. **start** - Start a PostgreSQL server instance
2. **stop** - Stop a running PostgreSQL server instance
3. **drop** - Stop and permanently delete an instance (removes all data)
4. **info** - Display instance information (status, connection URI, etc.)
5. **list** - List all PostgreSQL instances
6. **psql** - Open an interactive psql shell connected to an instance
7. **logs** - View PostgreSQL logs for debugging

### Start PostgreSQL

```bash
# Start with defaults (port 5432)
pg0 start

# Start with custom options
pg0 start --port 5433 --username myuser --password mypass --database myapp
```

### Stop PostgreSQL

```bash
pg0 stop
```

### Drop Instance

Permanently delete an instance and all its data:

```bash
# Drop the default instance
pg0 drop

# Drop a named instance
pg0 drop --name myapp
```

**Warning:** This command will stop the instance if running and delete all data. This action cannot be undone.

### Get Server Info

```bash
# Human-readable format
pg0 info

# JSON output
pg0 info -o json

# Info for a specific instance
pg0 info --name myapp
```

### List Instances

```bash
# List all instances
pg0 list

# JSON output
pg0 list -o json
```

### Open psql Shell

```bash
# Interactive shell
pg0 psql

# Run a single command
pg0 psql -c "SELECT version();"

# Run a SQL file
pg0 psql -f schema.sql
```

### View Logs

View PostgreSQL logs for debugging startup issues or errors:

```bash
# View all logs
pg0 logs

# View last 50 lines
pg0 logs -n 50

# Follow logs in real-time (like tail -f)
pg0 logs --follow

# Logs for a specific instance
pg0 logs --name myapp
```

Logs are stored in `~/.pg0/instances/<name>/data/log/`.

### Installing Extensions

#### pg_textsearch (BM25 full-text search)

[pg_textsearch](https://github.com/timescale/pg_textsearch) adds BM25-ranked full-text search to PostgreSQL. Install it into your pg0 instance with a single command (requires Xcode Command Line Tools on macOS, or `build-essential` on Linux):

```bash
curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/extensions/install-pgtextsearch.sh | bash
```

Install a specific version or target a named instance:

```bash
curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/extensions/install-pgtextsearch.sh | bash -s -- --version v0.5.1 --instance myapp
```

Then enable it:

```bash
pg0 psql -c "CREATE EXTENSION IF NOT EXISTS pg_textsearch;"
```

### Using pgvector

pgvector is pre-installed. Just enable it:

```bash
pg0 psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
```

Then use it for vector similarity search:

```sql
-- Create a table with vector column
CREATE TABLE items (id serial PRIMARY KEY, embedding vector(1536));

-- Insert vectors
INSERT INTO items (embedding) VALUES ('[0.1, 0.2, ...]');

-- Find similar vectors
SELECT * FROM items ORDER BY embedding <-> '[0.1, 0.2, ...]' LIMIT 5;
```

### Multiple Instances

Run multiple PostgreSQL servers simultaneously using named instances:

```bash
# Start multiple instances on different ports
pg0 start --name app1 --port 5432
pg0 start --name app2 --port 5433
pg0 start --name test --port 5434

# List all instances
pg0 list

# Get info for a specific instance
pg0 info --name app1

# Connect to a specific instance
pg0 psql --name app2

# Stop a specific instance
pg0 stop --name test

# Stop all (one by one)
pg0 stop --name app1
pg0 stop --name app2
```

Each instance has its own data directory at `~/.pg0/instances/<name>/data/`.

## Options

### Global Options

```
  -v, --verbose  Enable verbose logging
```

### Start Options

```
pg0 start [OPTIONS]

Options:
      --name <NAME>           Instance name [default: default]
  -p, --port <PORT>           Port to listen on [default: 5432]
  -d, --data-dir <DATA_DIR>   Data directory [default: ~/.pg0/instances/<name>/data]
  -u, --username <USERNAME>   Username [default: postgres]
  -P, --password <PASSWORD>   Password [default: postgres]
  -n, --database <DATABASE>   Database name [default: postgres]
  -c, --config <KEY=VALUE>    PostgreSQL config option (can repeat)
```

### PostgreSQL Configuration

pg0 applies optimized defaults for vector/AI workloads:
- `shared_buffers=256MB`
- `maintenance_work_mem=512MB` (faster index builds)
- `effective_cache_size=1GB`
- `max_parallel_maintenance_workers=4`
- `work_mem=64MB`

Override any setting with `-c`:

```bash
# Custom memory settings
pg0 start -c shared_buffers=512MB -c work_mem=128MB

# For larger workloads
pg0 start -c shared_buffers=1GB -c maintenance_work_mem=2GB
```

## How It Works

PostgreSQL and pgvector are **bundled directly** into the pg0 binary - no downloads required, works completely offline! On first start, pg0 extracts PostgreSQL and pgvector to `~/.pg0/installation/` and initializes the database.

Data is stored in `~/.pg0/instances/<name>/data/` (or your custom `--data-dir`) and persists between restarts.

## Runtime Dependencies

pg0 bundles PostgreSQL, pgvector, libxml2 and ICU directly into the binary, so it works on minimal systems without those libraries installed (including Ubuntu 25.10+ where the libxml2 SONAME was bumped to `.so.16`). A few common shared libraries still need to be present on the host because they are reused from the OS.

**macOS:** No additional dependencies required.

**Linux (Debian/Ubuntu):**
```bash
apt-get install libssl3 libgssapi-krb5-2 tzdata libreadline8
```

**Linux (Alpine):**
```bash
apk add icu-libs lz4-libs libxml2
```
(Alpine uses the musl pg0 binary, which dynamically links against system ICU and libxml2. See the support table below for compatible Alpine versions.)

### Why these dependencies?

The bundled PostgreSQL binaries are compiled with these features enabled:

| Library | Purpose | Bundled in pg0? |
|---------|---------|-----------------|
| libxml2 | XML data type and functions | Yes (Linux GNU only) |
| ICU (`libicu*`) | Unicode collation | Yes (Linux GNU only); Alpine uses system `icu-libs` |
| OpenSSL (`libssl`) | SSL/TLS connections | No - host-provided |
| GSSAPI (`libgssapi-krb5`) | Kerberos authentication | No - host-provided |
| LZ4 (`lz4-libs`) | WAL/TOAST compression | No - usually pre-installed |
| tzdata (`/usr/share/zoneinfo`) | Time zone data | No - host-provided |
| Readline (`libreadline`) | Interactive `pg0 psql` | No - host-provided |

Most desktop Linux distributions and macOS have these libraries pre-installed. You only need to install them manually in minimal Docker images or bare-metal servers.

## Tested and Supported Platforms

The table below reflects what we actually exercise via the docker tests in `docker-tests/` plus the platforms targeted by the release CI. Anything not in the table is best-effort: it may work, but we do not test it.

| Platform / Image | Architecture | Status | Notes |
|---|---|---|---|
| macOS (Apple Silicon, M1/M2/M3) | aarch64 | ✅ Supported | Released binary; built in CI |
| macOS (Intel) | x86_64 | ✅ Supported | Released binary; built in CI |
| Debian 12 (bookworm) | x86_64, aarch64 | ✅ Tested | `docker-tests/test_debian_*.sh` (python:3.11-slim) |
| Debian 13 (trixie) | x86_64, aarch64 | ✅ Expected to work | Same glibc / libxml2 ABI as bookworm |
| Ubuntu 22.04 (Jammy) | x86_64, aarch64 | ✅ Expected to work | glibc 2.35 baseline; matches release CI build host |
| Ubuntu 24.04 (Noble) | x86_64 | ✅ Tested | `docker-tests/test_ubuntu_amd64.sh` |
| Ubuntu 25.10 (Plucky) | x86_64 | ✅ Tested | `docker-tests/test_ubuntu_amd64.sh` - works thanks to bundled libxml2.so.2 + libicu70 |
| Ubuntu 26.04 (next LTS) | x86_64, aarch64 | ✅ Expected to work | Inherits libxml2 2.14 / ICU 76 from 25.10 |
| Alpine 3.20 | x86_64, aarch64 | ✅ Tested | `docker-tests/test_alpine_*.sh` (python:3.12-alpine3.20). Uses musl + system ICU 74 |
| Alpine 3.21 | x86_64, aarch64 | ✅ Expected to work | Same ICU 74 line as 3.20 (untested but ABI-compatible) |
| Alpine 3.22, 3.23+ | x86_64, aarch64 | ❌ Not supported | Ships ICU 76; the upstream theseus-rs musl PostgreSQL binary is built against ICU 74 and there is no compat package on Alpine. Use Alpine 3.20 or 3.21 instead |
| Windows 10/11 | x86_64 | ✅ Supported | Released binary; built in CI |
| NixOS | x86_64, aarch64 | ✅ Supported | Timezone pinned to UTC since v0.13.0 ([#11](https://github.com/vectorize-io/pg0/issues/11)) |
| Any environment that runs as root only (e.g. Google Colab, restricted containers) | any | ❌ Not supported | PostgreSQL refuses to run as root - see [Troubleshooting](#postgresql-cannot-run-as-root) |
| Linux with glibc < 2.35 | any | ⚠️ Auto-fallback | The install script switches to the statically-linked musl binary; pgvector is not available in that mode |

## Troubleshooting

### PostgreSQL Cannot Run as Root

PostgreSQL refuses to run as root for security reasons. If you see this error:

```
initdb: error: cannot be run as root
```

You need to run pg0 as a non-root user:

```bash
# Create a non-root user and run pg0
useradd -m pguser
su - pguser -c "pg0 start"
```

**Note:** This means pg0 won't work in environments that only allow root access, such as:
- Google Colab (runs as root)
- Some CI environments
- Restricted containers

See the [Docker](#docker) section for complete examples of running pg0 as a non-root user.

### Port Already in Use

If port 5432 is already in use, pg0 will automatically find an available port:

```bash
pg0 start --name second-instance
# Output: Port 5432 is in use, using port 54321 instead.
```

To use a specific port, specify it explicitly:

```bash
pg0 start --port 5433
```

## Build from Source

```bash
cargo build --release
```

The binary will be at `target/release/pg0`.

## Changelog

### 0.12.2
- Fix reproducible builds by committing `Cargo.lock` ([`28c51ec`](https://github.com/vectorize-io/pg0/commit/28c51ec))

### 0.12.1
- Fix data loss on restart after unclean shutdown ([#6](https://github.com/vectorize-io/pg0/issues/6), [`21e0f08`](https://github.com/vectorize-io/pg0/commit/21e0f08))

### 0.12.0
- Intel Mac (x86_64) support ([#5](https://github.com/vectorize-io/pg0/pull/5))
- Fix Python sdist build ([#4](https://github.com/vectorize-io/pg0/pull/4))

### 0.11.0
- Improved error handling and logging in Python SDK ([`b6cb333`](https://github.com/vectorize-io/pg0/commit/b6cb333))

### 0.10.0
- Bundled CLI binary in Python package ([#1](https://github.com/vectorize-io/pg0/pull/1))

### 0.9.0
- Bundled pgvector extension ([`5ee9fee`](https://github.com/vectorize-io/pg0/commit/5ee9fee))

### 0.8.0
- Bundled PostgreSQL binaries for offline use ([`a565d5b`](https://github.com/vectorize-io/pg0/commit/a565d5b))

### 0.7.0
- GLIBC 2.31 support ([`dd4755c`](https://github.com/vectorize-io/pg0/commit/dd4755c))

### 0.6.0
- ARM64 + Alpine Linux support ([`ebcd95d`](https://github.com/vectorize-io/pg0/commit/ebcd95d))
- `drop` command and Python/Node SDKs ([`24b75fa`](https://github.com/vectorize-io/pg0/commit/24b75fa))

### 0.2.0
- Multi-instance support ([`b3ac463`](https://github.com/vectorize-io/pg0/commit/b3ac463))

### 0.1.0
- Initial release

## License

MIT
