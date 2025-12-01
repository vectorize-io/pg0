"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.PostgreSQL = exports.Pg0AlreadyRunningError = exports.Pg0NotRunningError = exports.Pg0NotFoundError = exports.Pg0Error = void 0;
exports.listInstances = listInstances;
exports.listInstancesSync = listInstancesSync;
exports.start = start;
exports.startSync = startSync;
exports.stop = stop;
exports.stopSync = stopSync;
exports.info = info;
exports.infoSync = infoSync;
const child_process_1 = require("child_process");
const util_1 = require("util");
const execFileAsync = (0, util_1.promisify)(child_process_1.execFile);
class Pg0Error extends Error {
    constructor(message) {
        super(message);
        this.name = "Pg0Error";
    }
}
exports.Pg0Error = Pg0Error;
class Pg0NotFoundError extends Pg0Error {
    constructor() {
        super("pg0 binary not found. Install it with: " +
            "curl -fsSL https://raw.githubusercontent.com/vectorize-io/pg0/main/install.sh | bash");
        this.name = "Pg0NotFoundError";
    }
}
exports.Pg0NotFoundError = Pg0NotFoundError;
class Pg0NotRunningError extends Pg0Error {
    constructor(message = "PostgreSQL instance is not running") {
        super(message);
        this.name = "Pg0NotRunningError";
    }
}
exports.Pg0NotRunningError = Pg0NotRunningError;
class Pg0AlreadyRunningError extends Pg0Error {
    constructor(message = "PostgreSQL instance is already running") {
        super(message);
        this.name = "Pg0AlreadyRunningError";
    }
}
exports.Pg0AlreadyRunningError = Pg0AlreadyRunningError;
function findPg0() {
    const { execSync } = require("child_process");
    try {
        const result = execSync("which pg0", { encoding: "utf-8" }).trim();
        if (result)
            return result;
    }
    catch {
        // Try common paths
        const paths = [
            "/usr/local/bin/pg0",
            `${process.env.HOME}/.local/bin/pg0`,
            `${process.env.HOME}/bin/pg0`,
        ];
        for (const path of paths) {
            try {
                execSync(`test -x "${path}"`, { encoding: "utf-8" });
                return path;
            }
            catch {
                continue;
            }
        }
    }
    throw new Pg0NotFoundError();
}
async function runPg0(...args) {
    const pg0Path = findPg0();
    try {
        const result = await execFileAsync(pg0Path, args);
        return result;
    }
    catch (error) {
        const stderr = error.stderr || error.message || "";
        if (stderr.toLowerCase().includes("already running")) {
            throw new Pg0AlreadyRunningError(stderr);
        }
        else if (stderr.toLowerCase().includes("no running instance") ||
            stderr.toLowerCase().includes("not running")) {
            throw new Pg0NotRunningError(stderr);
        }
        throw new Pg0Error(stderr || `pg0 command failed`);
    }
}
function runPg0Sync(...args) {
    const pg0Path = findPg0();
    try {
        return (0, child_process_1.execFileSync)(pg0Path, args, { encoding: "utf-8" });
    }
    catch (error) {
        const stderr = error.stderr || error.message || "";
        if (stderr.toLowerCase().includes("already running")) {
            throw new Pg0AlreadyRunningError(stderr);
        }
        else if (stderr.toLowerCase().includes("no running instance") ||
            stderr.toLowerCase().includes("not running")) {
            throw new Pg0NotRunningError(stderr);
        }
        throw new Pg0Error(stderr || `pg0 command failed`);
    }
}
/**
 * Control a pg0 PostgreSQL instance.
 *
 * @example
 * ```typescript
 * const pg = new PostgreSQL({ port: 5433, database: "myapp" });
 * await pg.start();
 * console.log(await pg.getUri());
 * await pg.stop();
 * ```
 */
class PostgreSQL {
    constructor(options = {}) {
        this.name = options.name ?? "default";
        this.port = options.port ?? 5432;
        this.username = options.username ?? "postgres";
        this.password = options.password ?? "postgres";
        this.database = options.database ?? "postgres";
        this.dataDir = options.dataDir;
        this.config = options.config ?? {};
    }
    /**
     * Start the PostgreSQL instance.
     * @returns Instance info with connection details
     */
    async start() {
        const args = [
            "start",
            "--name", this.name,
            "--port", String(this.port),
            "--username", this.username,
            "--password", this.password,
            "--database", this.database,
        ];
        if (this.dataDir) {
            args.push("--data-dir", this.dataDir);
        }
        for (const [key, value] of Object.entries(this.config)) {
            args.push("-c", `${key}=${value}`);
        }
        await runPg0(...args);
        return this.info();
    }
    /**
     * Start the PostgreSQL instance (synchronous).
     * @returns Instance info with connection details
     */
    startSync() {
        const args = [
            "start",
            "--name", this.name,
            "--port", String(this.port),
            "--username", this.username,
            "--password", this.password,
            "--database", this.database,
        ];
        if (this.dataDir) {
            args.push("--data-dir", this.dataDir);
        }
        for (const [key, value] of Object.entries(this.config)) {
            args.push("-c", `${key}=${value}`);
        }
        runPg0Sync(...args);
        return this.infoSync();
    }
    /**
     * Stop the PostgreSQL instance.
     */
    async stop() {
        await runPg0("stop", "--name", this.name);
    }
    /**
     * Stop the PostgreSQL instance (synchronous).
     */
    stopSync() {
        runPg0Sync("stop", "--name", this.name);
    }
    /**
     * Get information about the PostgreSQL instance.
     */
    async info() {
        const { stdout } = await runPg0("info", "--name", this.name, "-o", "json");
        return JSON.parse(stdout);
    }
    /**
     * Get information about the PostgreSQL instance (synchronous).
     */
    infoSync() {
        const stdout = runPg0Sync("info", "--name", this.name, "-o", "json");
        return JSON.parse(stdout);
    }
    /**
     * Get the connection URI if running.
     */
    async getUri() {
        const info = await this.info();
        return info.uri;
    }
    /**
     * Get the connection URI if running (synchronous).
     */
    getUriSync() {
        const info = this.infoSync();
        return info.uri;
    }
    /**
     * Check if the instance is running.
     */
    async isRunning() {
        const info = await this.info();
        return info.running;
    }
    /**
     * Check if the instance is running (synchronous).
     */
    isRunningSync() {
        const info = this.infoSync();
        return info.running;
    }
    /**
     * Run psql with the given arguments.
     * @param args Arguments to pass to psql
     */
    async psql(...args) {
        return runPg0("psql", "--name", this.name, ...args);
    }
    /**
     * Run psql with the given arguments (synchronous).
     * @param args Arguments to pass to psql
     */
    psqlSync(...args) {
        return runPg0Sync("psql", "--name", this.name, ...args);
    }
    /**
     * Execute a SQL command and return the output.
     * @param sql SQL command to execute
     */
    async execute(sql) {
        const { stdout } = await this.psql("-c", sql);
        return stdout;
    }
    /**
     * Execute a SQL command and return the output (synchronous).
     * @param sql SQL command to execute
     */
    executeSync(sql) {
        return this.psqlSync("-c", sql);
    }
}
exports.PostgreSQL = PostgreSQL;
/**
 * List all pg0 instances.
 */
async function listInstances() {
    const { stdout } = await runPg0("list", "-o", "json");
    return JSON.parse(stdout);
}
/**
 * List all pg0 instances (synchronous).
 */
function listInstancesSync() {
    const stdout = runPg0Sync("list", "-o", "json");
    return JSON.parse(stdout);
}
/**
 * Start a PostgreSQL instance (convenience function).
 */
async function start(options = {}) {
    const pg = new PostgreSQL(options);
    return pg.start();
}
/**
 * Start a PostgreSQL instance (synchronous convenience function).
 */
function startSync(options = {}) {
    const pg = new PostgreSQL(options);
    return pg.startSync();
}
/**
 * Stop a PostgreSQL instance (convenience function).
 */
async function stop(name = "default") {
    await runPg0("stop", "--name", name);
}
/**
 * Stop a PostgreSQL instance (synchronous convenience function).
 */
function stopSync(name = "default") {
    runPg0Sync("stop", "--name", name);
}
/**
 * Get information about a PostgreSQL instance (convenience function).
 */
async function info(name = "default") {
    const { stdout } = await runPg0("info", "--name", name, "-o", "json");
    return JSON.parse(stdout);
}
/**
 * Get information about a PostgreSQL instance (synchronous convenience function).
 */
function infoSync(name = "default") {
    const stdout = runPg0Sync("info", "--name", name, "-o", "json");
    return JSON.parse(stdout);
}
exports.default = PostgreSQL;
