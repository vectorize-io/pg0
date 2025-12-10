# pg0 - Embedded PostgreSQL for Node.js

[![npm](https://badge.fury.io/js/@vectorize-io%2Fpg0.svg)](https://www.npmjs.com/package/@vectorize-io/pg0)

Embedded PostgreSQL with pgvector. No installation, no Docker, no configuration.

## Install

```bash
npm install @vectorize-io/pg0
```

## Usage

```typescript
import { Pg0 } from "@vectorize-io/pg0";

// Basic usage
const pg = new Pg0();
await pg.start();
console.log(await pg.getUri()); // postgresql://postgres:postgres@localhost:5432/postgres
await pg.execute("CREATE EXTENSION IF NOT EXISTS vector");
await pg.stop();

// Custom configuration
const pg = new Pg0({
  name: "myapp",
  port: 5433,
  username: "myuser",
  password: "mypass",
  database: "mydb",
  config: { shared_buffers: "512MB" }
});
await pg.start();
await pg.stop();

// Sync API also available
pg.startSync();
pg.stopSync();
```

## API

### Pg0 Class

| Method | Description |
|--------|-------------|
| `start()` / `startSync()` | Start PostgreSQL, returns `InstanceInfo` |
| `stop()` / `stopSync()` | Stop PostgreSQL |
| `drop()` / `dropSync()` | Stop and delete all data |
| `info()` / `infoSync()` | Get instance info |
| `execute(sql)` / `executeSync(sql)` | Run SQL query |
| `getUri()` / `getUriSync()` | Get connection URI |
| `isRunning()` / `isRunningSync()` | Check if running |

### Module Functions

```typescript
import { start, stop, drop, info, listInstances } from "@vectorize-io/pg0";

await start({ name: "default", port: 5432 });  // Start instance
await stop("default");                          // Stop instance
await drop("default");                          // Delete instance
await info("default");                          // Get instance info
await listInstances();                          // List all instances

// Sync versions: startSync, stopSync, dropSync, infoSync, listInstancesSync
```

## Links

- [GitHub](https://github.com/vectorize-io/pg0)
- [CLI Documentation](https://github.com/vectorize-io/pg0#readme)
