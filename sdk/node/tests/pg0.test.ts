import { describe, it, before, after } from "node:test";
import assert from "node:assert";
import {
  PostgreSQL,
  Pg0NotRunningError,
  Pg0AlreadyRunningError,
  start,
  stop,
  info,
  listInstances,
  startSync,
  stopSync,
  infoSync,
  listInstancesSync,
} from "../src";

// Use unique ports to avoid conflicts
const TEST_PORT = 15433;
const TEST_NAME = "node-test";

function cleanup() {
  try {
    stopSync(TEST_NAME);
  } catch (e) {
    // Ignore if not running
  }
}

describe("PostgreSQL class", () => {
  before(() => cleanup());
  after(() => cleanup());

  describe("start and stop", () => {
    after(() => cleanup());

    it("should start and stop PostgreSQL", async () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });

      // Start
      const startInfo = await pg.start();
      assert.strictEqual(startInfo.running, true);
      assert.strictEqual(startInfo.port, TEST_PORT);
      assert(startInfo.uri);
      assert(startInfo.uri.includes(`:${TEST_PORT}/`));

      // Stop
      await pg.stop();
      const stopInfo = await pg.info();
      assert.strictEqual(stopInfo.running, false);
    });
  });

  describe("sync methods", () => {
    after(() => cleanup());

    it("should start and stop PostgreSQL synchronously", () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });

      // Start
      const startInfo = pg.startSync();
      assert.strictEqual(startInfo.running, true);
      assert.strictEqual(startInfo.port, TEST_PORT);

      // Check running
      assert.strictEqual(pg.isRunningSync(), true);
      assert(pg.getUriSync());

      // Stop
      pg.stopSync();
      assert.strictEqual(pg.isRunningSync(), false);
    });
  });

  describe("execute SQL", () => {
    after(() => cleanup());

    it("should execute SQL commands", async () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
      await pg.start();

      try {
        // Execute a simple query
        const result = await pg.execute("SELECT 1 as num;");
        assert(result.includes("1"));

        // Create and query a table
        await pg.execute("CREATE TABLE test_table (id serial, name text);");
        await pg.execute("INSERT INTO test_table (name) VALUES ('hello');");
        const queryResult = await pg.execute("SELECT name FROM test_table;");
        assert(queryResult.includes("hello"));
      } finally {
        await pg.stop();
      }
    });
  });

  describe("custom credentials", () => {
    after(() => cleanup());

    it("should use custom username, password, database", async () => {
      const pg = new PostgreSQL({
        name: TEST_NAME,
        port: TEST_PORT,
        username: "testuser",
        password: "testpass",
        database: "testdb",
      });
      const info = await pg.start();

      try {
        assert(info.uri);
        assert(info.uri.includes("testuser"));
        assert(info.uri.includes("testpass"));
        assert(info.uri.includes("testdb"));
      } finally {
        await pg.stop();
      }
    });
  });

  describe("custom config", () => {
    after(() => cleanup());

    it("should apply custom PostgreSQL configuration", async () => {
      const pg = new PostgreSQL({
        name: TEST_NAME,
        port: TEST_PORT,
        config: { work_mem: "128MB" },
      });
      await pg.start();

      try {
        const result = await pg.execute("SHOW work_mem;");
        assert(result.includes("128MB"));
      } finally {
        await pg.stop();
      }
    });
  });

  describe("error handling", () => {
    after(() => cleanup());

    it("should throw Pg0AlreadyRunningError when starting twice", async () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
      await pg.start();

      try {
        await assert.rejects(async () => {
          await pg.start();
        }, Pg0AlreadyRunningError);
      } finally {
        await pg.stop();
      }
    });

    it("should throw Pg0NotRunningError when stopping non-running instance", async () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });

      await assert.rejects(async () => {
        await pg.stop();
      }, Pg0NotRunningError);
    });

    it("should return running=false for non-running instance", async () => {
      const pg = new PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
      const info = await pg.info();

      assert.strictEqual(info.running, false);
      assert.strictEqual(info.uri, undefined);
    });
  });
});

describe("Convenience functions", () => {
  before(() => cleanup());
  after(() => cleanup());

  describe("async functions", () => {
    after(() => cleanup());

    it("should start, get info, and stop", async () => {
      const startInfo = await start({ name: TEST_NAME, port: TEST_PORT });
      assert.strictEqual(startInfo.running, true);

      const infoResult = await info(TEST_NAME);
      assert.strictEqual(infoResult.running, true);
      assert.strictEqual(infoResult.port, TEST_PORT);

      await stop(TEST_NAME);
      const afterStop = await info(TEST_NAME);
      assert.strictEqual(afterStop.running, false);
    });
  });

  describe("sync functions", () => {
    after(() => cleanup());

    it("should start, get info, and stop synchronously", () => {
      const startInfo = startSync({ name: TEST_NAME, port: TEST_PORT });
      assert.strictEqual(startInfo.running, true);

      const infoResult = infoSync(TEST_NAME);
      assert.strictEqual(infoResult.running, true);

      stopSync(TEST_NAME);
      const afterStop = infoSync(TEST_NAME);
      assert.strictEqual(afterStop.running, false);
    });
  });

  describe("list instances", () => {
    after(() => cleanup());

    it("should list running instances", async () => {
      await start({ name: TEST_NAME, port: TEST_PORT });

      try {
        const instances = await listInstances();
        const names = instances.map((i) => i.name);
        assert(names.includes(TEST_NAME));
      } finally {
        await stop(TEST_NAME);
      }
    });

    it("should list running instances synchronously", () => {
      startSync({ name: TEST_NAME, port: TEST_PORT });

      try {
        const instances = listInstancesSync();
        const names = instances.map((i) => i.name);
        assert(names.includes(TEST_NAME));
      } finally {
        stopSync(TEST_NAME);
      }
    });
  });
});
