# pg0

**Zero-dependency embedded PostgreSQL** - Run PostgreSQL locally without installation. A single binary that downloads and manages PostgreSQL for you.

Includes **pgvector** for AI/vector workloads out of the box.

## Features

- **Zero dependencies** - single binary, no installation required
- **Embedded PostgreSQL 16** with pgvector pre-installed
- Works on macOS (Apple Silicon), Linux (x86_64), and Windows (x64)
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

### Get Server Info

```bash
# Human-readable format
pg0 info

# JSON output
pg0 info -o json
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
  -p, --port <PORT>          Port to listen on [default: 5432]
  -d, --data-dir <DATA_DIR>  Data directory [default: ~/.pg0/data]
  -u, --username <USERNAME>  Username [default: postgres]
  -P, --password <PASSWORD>  Password [default: postgres]
  -n, --database <DATABASE>  Database name [default: postgres]
```

## How It Works

On first run, pg0 downloads PostgreSQL from [theseus-rs](https://github.com/theseus-rs/postgresql-binaries) and pgvector from pre-compiled binaries. These are cached in `~/.pg0/installation/` for subsequent runs.

Data is stored in `~/.pg0/data/` (or your custom `--data-dir`) and persists between restarts.

## Build from Source

```bash
cargo build --release
```

The binary will be at `target/release/pg0`.

## License

MIT
