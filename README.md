# embedded-postgres-cli

Run PostgreSQL locally without installation. Includes **pgvector** for AI/vector workloads out of the box.

## Features

- Zero dependencies - just download and run
- PostgreSQL 16 with pgvector pre-installed
- Works on macOS (Intel & Apple Silicon) and Linux (x86_64 & ARM64)
- Bundled `psql` client - no separate installation needed
- Data persists between restarts

## Installation

```bash
curl -fsSL https://raw.githubusercontent.com/vectorize-io/embedded-pg-cli/main/install.sh | bash
```

Or with a custom install directory:

```bash
INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/vectorize-io/embedded-pg-cli/main/install.sh | bash
```

## Quick Start

```bash
# Start PostgreSQL
embedded-postgres start

# Connect with psql
embedded-postgres psql

# Use pgvector
embedded-postgres psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
embedded-postgres psql -c "CREATE TABLE items (embedding vector(3));"
embedded-postgres psql -c "INSERT INTO items VALUES ('[1,2,3]');"

# Stop when done
embedded-postgres stop
```

## Usage

### Start PostgreSQL

```bash
# Start with defaults (port 5432, PostgreSQL 16)
embedded-postgres start

# Start with custom options
embedded-postgres start --port 5433 --username myuser --password mypass --database myapp
```

### Stop PostgreSQL

```bash
embedded-postgres stop
```

### Check Status

```bash
embedded-postgres status
```

### Get Connection URI

```bash
embedded-postgres uri
# Output: postgresql://postgres:postgres@localhost:5432/postgres
```

### Open psql Shell

```bash
# Interactive shell
embedded-postgres psql

# Run a single command
embedded-postgres psql -c "SELECT version();"

# Run a SQL file
embedded-postgres psql -f schema.sql
```

### Using pgvector

pgvector is pre-installed. Just enable it:

```bash
embedded-postgres psql -c "CREATE EXTENSION IF NOT EXISTS vector;"
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

### Install Additional Extensions

For extensions beyond pgvector:

```bash
# List available extensions
embedded-postgres list-extensions

# Install an extension
embedded-postgres install-extension <name>
```

## Options

### Global Options

```
  -v, --verbose  Enable verbose logging
```

### Start Options

```
embedded-postgres start [OPTIONS]

Options:
  -p, --port <PORT>          Port to listen on [default: 5432]
  -d, --data-dir <DATA_DIR>  Data directory [default: ~/.embedded-postgres/data]
  -u, --username <USERNAME>  Username [default: postgres]
  -P, --password <PASSWORD>  Password [default: postgres]
  -n, --database <DATABASE>  Database name [default: postgres]
```

## How It Works

On first run, the CLI downloads a pre-built PostgreSQL 16 + pgvector bundle (~50MB) from GitHub releases. This is cached in `~/.embedded-postgres/installation/` for subsequent runs.

Data is stored in `~/.embedded-postgres/data/` and persists between restarts.

## Build from Source

```bash
./build.sh
```

The binary will be at `target/release/embedded-postgres`.

## License

MIT
