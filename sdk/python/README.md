# pg0 - PostgreSQL for Python

[![PyPI](https://badge.fury.io/py/pg0-embedded.svg)](https://pypi.org/project/pg0-embedded/)

Zero-config PostgreSQL with pgvector. No installation, no Docker, no configuration.

## Install

```bash
pip install pg0-embedded
```

## Usage

```python
from pg0 import Pg0

# Basic usage
with Pg0() as pg:
    print(pg.uri)  # postgresql://postgres:postgres@localhost:5432/postgres
    pg.execute("CREATE EXTENSION IF NOT EXISTS vector")
    pg.execute("SELECT version()")

# Custom configuration
pg = Pg0(
    name="myapp",
    port=5433,
    username="myuser",
    password="mypass",
    database="mydb",
    config={"shared_buffers": "512MB"}
)
pg.start()
pg.stop()
```

## API

### Pg0 Class

| Method | Description |
|--------|-------------|
| `start()` | Start PostgreSQL, returns `InstanceInfo` |
| `stop()` | Stop PostgreSQL |
| `drop()` | Stop and delete all data |
| `info()` | Get instance info |
| `execute(sql)` | Run SQL query |
| `uri` | Connection URI (property) |
| `running` | Check if running (property) |

### Module Functions

```python
import pg0

pg0.start(name="default", port=5432, ...)  # Start instance
pg0.stop(name="default")                    # Stop instance
pg0.drop(name="default")                    # Delete instance
pg0.info(name="default")                    # Get instance info
pg0.list_instances()                        # List all instances
```

### Getting Connection URI

```python
from pg0 import Pg0

pg = Pg0()
pg.start()

# Using the uri property
print(pg.uri)  # postgresql://postgres:postgres@localhost:5432/postgres

# Or using info()
info = pg.info()
print(info.uri)  # postgresql://postgres:postgres@localhost:5432/postgres
print(info.port)  # 5432
print(info.username)  # postgres
print(info.database)  # postgres
```

## Links

- [GitHub](https://github.com/vectorize-io/pg0)
- [CLI Documentation](https://github.com/vectorize-io/pg0#readme)
