# pg0

**Zero-dependency embedded PostgreSQL** - Run PostgreSQL locally without installation. A single binary that downloads and manages PostgreSQL for you.

Includes **pgvector** for AI/vector workloads out of the box.

## Features

- **Zero dependencies** - single binary, no installation required
- **Embedded PostgreSQL 16** with pgvector pre-installed
- **Multiple instances** - run multiple PostgreSQL servers simultaneously
- Works on macOS (Apple Silicon), Linux (x86_64, statically linked), and Windows (x64)
- Bundled `psql` client - no separate installation needed
- Data persists between restarts

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
```

Or with a custom install directory:

```bash
INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash
```

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

### Install Additional Extensions

For extensions beyond pgvector:

```bash
# List available extensions
pg0 list-extensions

# Install an extension
pg0 install-extension <name>
```

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

On first run, pg0 downloads PostgreSQL from [theseus-rs](https://github.com/theseus-rs/postgresql-binaries) and pgvector from pre-compiled binaries. These are cached in `~/.pg0/installation/` for subsequent runs.

Data is stored in `~/.pg0/instances/<name>/data/` (or your custom `--data-dir`) and persists between restarts.

## Build from Source

```bash
cargo build --release
```

The binary will be at `target/release/pg0`.

## License

MIT
