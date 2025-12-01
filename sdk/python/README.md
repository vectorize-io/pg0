# pg0 - Embedded PostgreSQL for Python

Zero-config PostgreSQL with pgvector support. Just `pip install` and go.

## Installation

```bash
pip install pg0-embedded
```

## Quick Start

```python
from pg0 import Pg0

# Start PostgreSQL (auto-installs on first run)
pg = Pg0()
pg.start()

print(pg.uri)  # postgresql://postgres:postgres@localhost:5432/postgres

pg.stop()
```

## Context Manager

```python
from pg0 import Pg0

with Pg0() as pg:
    print(pg.uri)
    pg.execute("CREATE EXTENSION IF NOT EXISTS vector")
    pg.execute("SELECT version()")
# Automatically stopped
```

## Custom Configuration

```python
from pg0 import Pg0

pg = Pg0(
    port=5433,
    username="myuser",
    password="mypass",
    database="mydb",
    config={
        "shared_buffers": "512MB",
        "maintenance_work_mem": "1GB",
    }
)

with pg:
    print(pg.uri)
```

## Multiple Instances

```python
from pg0 import Pg0, list_instances

# Run multiple PostgreSQL instances
app = Pg0(name="app", port=5432)
test = Pg0(name="test", port=5433)

app.start()
test.start()

for instance in list_instances():
    print(f"{instance.name}: {instance.uri}")

app.stop()
test.stop()
```

## API Reference

### Pg0 Class

```python
pg = Pg0(
    name="default",      # Instance name
    port=5432,           # Port
    username="postgres", # Username
    password="postgres", # Password
    database="postgres", # Database
    data_dir=None,       # Custom data directory
    config={},           # PostgreSQL config options
)

pg.start()        # Start PostgreSQL -> InstanceInfo
pg.stop()         # Stop PostgreSQL
pg.info()         # Get instance info -> InstanceInfo
pg.uri            # Connection URI (property)
pg.running        # Is running (property)
pg.execute(sql)   # Execute SQL -> str
pg.psql(*args)    # Run psql command
```

### Module Functions

```python
import pg0

pg0.start(port=5432, ...)   # Start instance -> InstanceInfo
pg0.stop(name="default")    # Stop instance
pg0.info(name="default")    # Get info -> InstanceInfo
pg0.list_instances()        # List all -> [InstanceInfo]
pg0.install(version=None)   # Install pg0 binary
```

### InstanceInfo

```python
info.name       # Instance name
info.running    # Is running
info.pid        # Process ID
info.port       # Port
info.uri        # Connection URI
info.username   # Username
info.database   # Database
info.data_dir   # Data directory
```
