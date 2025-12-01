# pg0 - Embedded PostgreSQL for Node.js

Zero-config PostgreSQL with pgvector support. Just `npm install` and go.

## Installation

```bash
npm install @vectorize-io/pg0
```

## Quick Start

```typescript
import { Pg0 } from "@vectorize-io/pg0";

// Start PostgreSQL (auto-installs on first run)
const pg = new Pg0();
await pg.start();

console.log(await pg.getUri()); // postgresql://postgres:postgres@localhost:5432/postgres

await pg.stop();
```

## Synchronous API

```typescript
import { Pg0 } from "@vectorize-io/pg0";

const pg = new Pg0();
pg.startSync();
console.log(pg.getUriSync());
pg.stopSync();
```

## Custom Configuration

```typescript
import { Pg0 } from "@vectorize-io/pg0";

const pg = new Pg0({
  port: 5433,
  username: "myuser",
  password: "mypass",
  database: "mydb",
  config: {
    shared_buffers: "512MB",
    maintenance_work_mem: "1GB",
  },
});

await pg.start();
console.log(await pg.getUri());
await pg.stop();
```

## Multiple Instances

```typescript
import { Pg0, listInstances } from "@vectorize-io/pg0";

const app = new Pg0({ name: "app", port: 5432 });
const test = new Pg0({ name: "test", port: 5433 });

await app.start();
await test.start();

for (const instance of await listInstances()) {
  console.log(`${instance.name}: ${instance.uri}`);
}

await app.stop();
await test.stop();
```

## API Reference

### Pg0 Class

```typescript
const pg = new Pg0({
  name: "default",      // Instance name
  port: 5432,           // Port
  username: "postgres", // Username
  password: "postgres", // Password
  database: "postgres", // Database
  dataDir: undefined,   // Custom data directory
  config: {},           // PostgreSQL config options
});

// Async methods
await pg.start();       // Start PostgreSQL -> InstanceInfo
await pg.stop();        // Stop PostgreSQL
await pg.info();        // Get instance info -> InstanceInfo
await pg.getUri();      // Connection URI
await pg.isRunning();   // Is running
await pg.execute(sql);  // Execute SQL -> string

// Sync methods
pg.startSync();
pg.stopSync();
pg.infoSync();
pg.getUriSync();
pg.isRunningSync();
pg.executeSync(sql);
```

### Module Functions

```typescript
import { start, stop, info, listInstances, install } from "@vectorize-io/pg0";

// Async
await start({ port: 5432 });  // Start -> InstanceInfo
await stop("default");        // Stop
await info("default");        // Info -> InstanceInfo
await listInstances();        // List all -> InstanceInfo[]
await install();              // Install pg0 binary

// Sync
startSync({ port: 5432 });
stopSync("default");
infoSync("default");
listInstancesSync();
installSync();
```

### InstanceInfo

```typescript
interface InstanceInfo {
  name: string;
  running: boolean;
  pid?: number;
  port?: number;
  version?: string;
  username?: string;
  database?: string;
  data_dir?: string;
  uri?: string;
}
```
