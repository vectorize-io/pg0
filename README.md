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

| Platform | Architecture | Binary |
|----------|--------------|--------|
| macOS | Apple Silicon (M1/M2/M3) | `pg0-macos-arm64` |
| Linux | x86_64 (glibc) | `pg0-linux-amd64-gnu` |
| Linux | x86_64 (musl/Alpine) | `pg0-linux-amd64-musl` |
| Linux | ARM64 (glibc) | `pg0-linux-arm64-gnu` |
| Linux | ARM64 (musl/Alpine) | `pg0-linux-arm64-musl` |
| Windows | x64 | `pg0-windows-amd64.exe` |

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
print(pg.uri)  # postgresql://postgres:postgres@localhost:5432/postgres

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
# or: python:3.11-slim, ubuntu:22.04, etc.

# Install required dependencies
RUN apt-get update && apt-get install -y \
    curl \
    libxml2 \
    libssl3 \
    libgssapi-krb5-2 \
    && apt-get install -y libicu72 || apt-get install -y libicu74 || apt-get install -y libicu* \
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
# Debian/Ubuntu
docker run --rm -it python:3.11-slim bash -c '
  apt-get update -qq &&
  apt-get install -y curl libxml2 libssl3 libgssapi-krb5-2 libicu72 &&
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

pg0 bundles PostgreSQL but requires some shared libraries at runtime. These are typically pre-installed on most systems, but may need to be added in minimal environments like Docker.

**macOS:** No additional dependencies required.

**Linux (Debian/Ubuntu):**
```bash
apt-get install libxml2 libssl3 libgssapi-krb5-2
```

**Linux (Alpine):**
```bash
apk add icu-libs lz4-libs libxml2
```

### Why these dependencies?

The bundled PostgreSQL binaries are compiled with these features enabled:

| Library | Purpose | Can disable? |
|---------|---------|--------------|
| OpenSSL (`libssl`) | SSL/TLS connections | Not recommended |
| GSSAPI (`libgssapi-krb5`) | Kerberos authentication | Rarely needed locally |
| libxml2 | XML data type and functions | Rarely needed |
| ICU (`icu-libs`) | Unicode collation (Alpine only) | glibc builds don't need it |
| LZ4 (`lz4-libs`) | WAL/TOAST compression | Small impact |

Most desktop Linux distributions and macOS have these libraries pre-installed. You only need to install them manually in minimal Docker images or bare-metal servers.

## Troubleshooting

### PostgreSQL Cannot Run as Root

PostgreSQL refuses to run as root for security reasons. If you see permission errors in Docker or Linux:

```bash
# Create a non-root user
useradd -m pguser
su - pguser -c "pg0 start"
```

See the [Docker](#docker) section for complete examples.

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

## License

MIT
