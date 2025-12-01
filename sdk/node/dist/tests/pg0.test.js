"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const node_test_1 = require("node:test");
const node_assert_1 = __importDefault(require("node:assert"));
const src_1 = require("../src");
// Use unique ports to avoid conflicts
const TEST_PORT = 15433;
const TEST_NAME = "node-test";
function cleanup() {
    try {
        (0, src_1.stopSync)(TEST_NAME);
    }
    catch (e) {
        // Ignore if not running
    }
}
(0, node_test_1.describe)("PostgreSQL class", () => {
    (0, node_test_1.before)(() => cleanup());
    (0, node_test_1.after)(() => cleanup());
    (0, node_test_1.describe)("start and stop", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should start and stop PostgreSQL", async () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            // Start
            const startInfo = await pg.start();
            node_assert_1.default.strictEqual(startInfo.running, true);
            node_assert_1.default.strictEqual(startInfo.port, TEST_PORT);
            (0, node_assert_1.default)(startInfo.uri);
            (0, node_assert_1.default)(startInfo.uri.includes(`:${TEST_PORT}/`));
            // Stop
            await pg.stop();
            const stopInfo = await pg.info();
            node_assert_1.default.strictEqual(stopInfo.running, false);
        });
    });
    (0, node_test_1.describe)("sync methods", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should start and stop PostgreSQL synchronously", () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            // Start
            const startInfo = pg.startSync();
            node_assert_1.default.strictEqual(startInfo.running, true);
            node_assert_1.default.strictEqual(startInfo.port, TEST_PORT);
            // Check running
            node_assert_1.default.strictEqual(pg.isRunningSync(), true);
            (0, node_assert_1.default)(pg.getUriSync());
            // Stop
            pg.stopSync();
            node_assert_1.default.strictEqual(pg.isRunningSync(), false);
        });
    });
    (0, node_test_1.describe)("execute SQL", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should execute SQL commands", async () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            await pg.start();
            try {
                // Execute a simple query
                const result = await pg.execute("SELECT 1 as num;");
                (0, node_assert_1.default)(result.includes("1"));
                // Create and query a table
                await pg.execute("CREATE TABLE test_table (id serial, name text);");
                await pg.execute("INSERT INTO test_table (name) VALUES ('hello');");
                const queryResult = await pg.execute("SELECT name FROM test_table;");
                (0, node_assert_1.default)(queryResult.includes("hello"));
            }
            finally {
                await pg.stop();
            }
        });
    });
    (0, node_test_1.describe)("custom credentials", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should use custom username, password, database", async () => {
            const pg = new src_1.PostgreSQL({
                name: TEST_NAME,
                port: TEST_PORT,
                username: "testuser",
                password: "testpass",
                database: "testdb",
            });
            const info = await pg.start();
            try {
                (0, node_assert_1.default)(info.uri);
                (0, node_assert_1.default)(info.uri.includes("testuser"));
                (0, node_assert_1.default)(info.uri.includes("testpass"));
                (0, node_assert_1.default)(info.uri.includes("testdb"));
            }
            finally {
                await pg.stop();
            }
        });
    });
    (0, node_test_1.describe)("custom config", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should apply custom PostgreSQL configuration", async () => {
            const pg = new src_1.PostgreSQL({
                name: TEST_NAME,
                port: TEST_PORT,
                config: { work_mem: "128MB" },
            });
            await pg.start();
            try {
                const result = await pg.execute("SHOW work_mem;");
                (0, node_assert_1.default)(result.includes("128MB"));
            }
            finally {
                await pg.stop();
            }
        });
    });
    (0, node_test_1.describe)("error handling", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should throw Pg0AlreadyRunningError when starting twice", async () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            await pg.start();
            try {
                await node_assert_1.default.rejects(async () => {
                    await pg.start();
                }, src_1.Pg0AlreadyRunningError);
            }
            finally {
                await pg.stop();
            }
        });
        (0, node_test_1.it)("should throw Pg0NotRunningError when stopping non-running instance", async () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            await node_assert_1.default.rejects(async () => {
                await pg.stop();
            }, src_1.Pg0NotRunningError);
        });
        (0, node_test_1.it)("should return running=false for non-running instance", async () => {
            const pg = new src_1.PostgreSQL({ name: TEST_NAME, port: TEST_PORT });
            const info = await pg.info();
            node_assert_1.default.strictEqual(info.running, false);
            node_assert_1.default.strictEqual(info.uri, undefined);
        });
    });
});
(0, node_test_1.describe)("Convenience functions", () => {
    (0, node_test_1.before)(() => cleanup());
    (0, node_test_1.after)(() => cleanup());
    (0, node_test_1.describe)("async functions", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should start, get info, and stop", async () => {
            const startInfo = await (0, src_1.start)({ name: TEST_NAME, port: TEST_PORT });
            node_assert_1.default.strictEqual(startInfo.running, true);
            const infoResult = await (0, src_1.info)(TEST_NAME);
            node_assert_1.default.strictEqual(infoResult.running, true);
            node_assert_1.default.strictEqual(infoResult.port, TEST_PORT);
            await (0, src_1.stop)(TEST_NAME);
            const afterStop = await (0, src_1.info)(TEST_NAME);
            node_assert_1.default.strictEqual(afterStop.running, false);
        });
    });
    (0, node_test_1.describe)("sync functions", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should start, get info, and stop synchronously", () => {
            const startInfo = (0, src_1.startSync)({ name: TEST_NAME, port: TEST_PORT });
            node_assert_1.default.strictEqual(startInfo.running, true);
            const infoResult = (0, src_1.infoSync)(TEST_NAME);
            node_assert_1.default.strictEqual(infoResult.running, true);
            (0, src_1.stopSync)(TEST_NAME);
            const afterStop = (0, src_1.infoSync)(TEST_NAME);
            node_assert_1.default.strictEqual(afterStop.running, false);
        });
    });
    (0, node_test_1.describe)("list instances", () => {
        (0, node_test_1.after)(() => cleanup());
        (0, node_test_1.it)("should list running instances", async () => {
            await (0, src_1.start)({ name: TEST_NAME, port: TEST_PORT });
            try {
                const instances = await (0, src_1.listInstances)();
                const names = instances.map((i) => i.name);
                (0, node_assert_1.default)(names.includes(TEST_NAME));
            }
            finally {
                await (0, src_1.stop)(TEST_NAME);
            }
        });
        (0, node_test_1.it)("should list running instances synchronously", () => {
            (0, src_1.startSync)({ name: TEST_NAME, port: TEST_PORT });
            try {
                const instances = (0, src_1.listInstancesSync)();
                const names = instances.map((i) => i.name);
                (0, node_assert_1.default)(names.includes(TEST_NAME));
            }
            finally {
                (0, src_1.stopSync)(TEST_NAME);
            }
        });
    });
});
