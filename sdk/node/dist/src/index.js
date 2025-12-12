"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.PostgreSQL = exports.Pg0 = exports.Pg0AlreadyRunningError = exports.Pg0NotRunningError = exports.Pg0NotFoundError = exports.Pg0Error = void 0;
exports.install = install;
exports.installSync = installSync;
exports.listInstances = listInstances;
exports.listInstancesSync = listInstancesSync;
exports.listExtensions = listExtensions;
exports.listExtensionsSync = listExtensionsSync;
exports.start = start;
exports.startSync = startSync;
exports.stop = stop;
exports.stopSync = stopSync;
exports.drop = drop;
exports.dropSync = dropSync;
exports.info = info;
exports.infoSync = infoSync;
const child_process_1 = require("child_process");
const fs_1 = require("fs");
const os_1 = require("os");
const path_1 = require("path");
const util_1 = require("util");
const execFileAsync = (0, util_1.promisify)(child_process_1.execFile);
const PG0_REPO = "vectorize-io/pg0";
const INSTALL_SCRIPT_URL = `https://raw.githubusercontent.com/${PG0_REPO}/main/install.sh`;
class Pg0Error extends Error {
    constructor(message) {
        super(message);
        this.name = "Pg0Error";
    }
}
exports.Pg0Error = Pg0Error;
class Pg0NotFoundError extends Pg0Error {
    constructor(message = "pg0 binary not found") {
        super(message);
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
function getInstallDir() {
    if (process.platform === "win32") {
        const base = process.env.LOCALAPPDATA || (0, path_1.join)((0, os_1.homedir)(), "AppData", "Local");
        return (0, path_1.join)(base, "pg0", "bin");
    }
    return (0, path_1.join)((0, os_1.homedir)(), ".local", "bin");
}
/**
 * Install the pg0 binary using the official install script.
 * @param force Force reinstall even if already installed
 * @returns Path to installed binary
 */
async function install(force = false) {
    const installDir = getInstallDir();
    const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
    const binaryPath = (0, path_1.join)(installDir, binaryName);
    if ((0, fs_1.existsSync)(binaryPath) && !force) {
        return binaryPath;
    }
    // Use the official install script which handles:
    // - Platform detection (including old glibc fallback to musl)
    // - Intel Mac Rosetta handling
    // - Proper binary naming
    if (process.platform === "win32") {
        throw new Pg0NotFoundError("Auto-install not supported on Windows. " +
            "Please download pg0 manually from https://github.com/vectorize-io/pg0/releases");
    }
    console.log("Installing pg0 using official install script...");
    return new Promise((resolve, reject) => {
        const { exec } = require("child_process");
        exec(`curl -fsSL ${INSTALL_SCRIPT_URL} | bash`, { timeout: 120000 }, (error, stdout, stderr) => {
            if (error) {
                reject(new Pg0NotFoundError(`Install script failed: ${stderr || error.message}`));
                return;
            }
            // Verify installation
            if ((0, fs_1.existsSync)(binaryPath)) {
                resolve(binaryPath);
                return;
            }
            // Check if installed to a different location via PATH
            try {
                const which = (0, child_process_1.execSync)("which pg0", { encoding: "utf-8" }).trim();
                if (which) {
                    resolve(which);
                    return;
                }
            }
            catch {
                // Not in PATH
            }
            reject(new Pg0NotFoundError("Install script succeeded but pg0 binary not found"));
        });
    });
}
/**
 * Install the pg0 binary synchronously using the official install script.
 */
function installSync(force = false) {
    const installDir = getInstallDir();
    const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
    const binaryPath = (0, path_1.join)(installDir, binaryName);
    if ((0, fs_1.existsSync)(binaryPath) && !force) {
        return binaryPath;
    }
    // Use the official install script
    if (process.platform === "win32") {
        throw new Pg0NotFoundError("Auto-install not supported on Windows. " +
            "Please download pg0 manually from https://github.com/vectorize-io/pg0/releases");
    }
    console.log("Installing pg0 using official install script...");
    try {
        (0, child_process_1.execSync)(`curl -fsSL ${INSTALL_SCRIPT_URL} | bash`, {
            encoding: "utf-8",
            timeout: 120000,
        });
        // Verify installation
        if ((0, fs_1.existsSync)(binaryPath)) {
            return binaryPath;
        }
        // Check if installed to a different location via PATH
        try {
            const which = (0, child_process_1.execSync)("which pg0", { encoding: "utf-8" }).trim();
            if (which) {
                return which;
            }
        }
        catch {
            // Not in PATH
        }
        throw new Pg0NotFoundError("Install script succeeded but pg0 binary not found");
    }
    catch (e) {
        if (e instanceof Pg0NotFoundError)
            throw e;
        throw new Pg0NotFoundError(`Failed to install pg0: ${e.message || e}`);
    }
}
function findPg0Sync() {
    // Check PATH
    const { execSync } = require("child_process");
    try {
        const result = execSync("which pg0 2>/dev/null || where pg0 2>nul", { encoding: "utf-8" });
        if (result.trim())
            return result.trim().split("\n")[0];
    }
    catch {
        // Not in PATH
    }
    // Check install location
    const installDir = getInstallDir();
    const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
    const binaryPath = (0, path_1.join)(installDir, binaryName);
    if ((0, fs_1.existsSync)(binaryPath)) {
        return binaryPath;
    }
    // Auto-install
    return installSync();
}
async function findPg0() {
    // Check install location first (faster than which)
    const installDir = getInstallDir();
    const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
    const binaryPath = (0, path_1.join)(installDir, binaryName);
    if ((0, fs_1.existsSync)(binaryPath)) {
        return binaryPath;
    }
    // Check PATH
    const { execSync } = require("child_process");
    try {
        const result = execSync("which pg0 2>/dev/null || where pg0 2>nul", { encoding: "utf-8" });
        if (result.trim())
            return result.trim().split("\n")[0];
    }
    catch {
        // Not in PATH
    }
    // Auto-install
    return install();
}
async function runPg0(...args) {
    const pg0Path = await findPg0();
    try {
        return await execFileAsync(pg0Path, args);
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
        throw new Pg0Error(stderr || "pg0 command failed");
    }
}
function runPg0Sync(...args) {
    const pg0Path = findPg0Sync();
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
        throw new Pg0Error(stderr || "pg0 command failed");
    }
}
/**
 * Embedded PostgreSQL instance.
 *
 * @example
 * ```typescript
 * import { Pg0 } from "pg0";
 *
 * const pg = new Pg0();
 * await pg.start();
 * console.log(await pg.getUri());
 * await pg.stop();
 * ```
 */
class Pg0 {
    name;
    port;
    username;
    password;
    database;
    dataDir;
    config;
    constructor(options = {}) {
        this.name = options.name ?? "default";
        this.port = options.port ?? 5432;
        this.username = options.username ?? "postgres";
        this.password = options.password ?? "postgres";
        this.database = options.database ?? "postgres";
        this.dataDir = options.dataDir;
        this.config = options.config ?? {};
    }
    /** Start the PostgreSQL instance. */
    async start() {
        const args = this._buildStartArgs();
        await runPg0(...args);
        return this.info();
    }
    /** Start the PostgreSQL instance (synchronous). */
    startSync() {
        const args = this._buildStartArgs();
        runPg0Sync(...args);
        return this.infoSync();
    }
    _buildStartArgs() {
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
        return args;
    }
    /** Stop the PostgreSQL instance. */
    async stop() {
        try {
            await runPg0("stop", "--name", this.name);
        }
        catch (e) {
            // Ignore "not running" errors
            if (!(e instanceof Pg0NotRunningError))
                throw e;
        }
    }
    /** Stop the PostgreSQL instance (synchronous). */
    stopSync() {
        try {
            runPg0Sync("stop", "--name", this.name);
        }
        catch (e) {
            // Ignore "not running" errors
            if (!(e instanceof Pg0NotRunningError))
                throw e;
        }
    }
    /** Drop the PostgreSQL instance (stop if running, delete all data). */
    async drop(force = true) {
        const args = ["drop", "--name", this.name];
        if (force)
            args.push("--force");
        try {
            await runPg0(...args);
        }
        catch {
            // Ignore errors
        }
    }
    /** Drop the PostgreSQL instance (synchronous). */
    dropSync(force = true) {
        const args = ["drop", "--name", this.name];
        if (force)
            args.push("--force");
        try {
            runPg0Sync(...args);
        }
        catch {
            // Ignore errors
        }
    }
    /** Get information about the PostgreSQL instance. */
    async info() {
        const { stdout } = await runPg0("info", "--name", this.name, "-o", "json");
        return JSON.parse(stdout);
    }
    /** Get information about the PostgreSQL instance (synchronous). */
    infoSync() {
        const stdout = runPg0Sync("info", "--name", this.name, "-o", "json");
        return JSON.parse(stdout);
    }
    /** Get the connection URI if running. */
    async getUri() {
        return (await this.info()).uri;
    }
    /** Get the connection URI if running (synchronous). */
    getUriSync() {
        return this.infoSync().uri;
    }
    /** Check if the instance is running. */
    async isRunning() {
        return (await this.info()).running;
    }
    /** Check if the instance is running (synchronous). */
    isRunningSync() {
        return this.infoSync().running;
    }
    /** Run psql with the given arguments. */
    async psql(...args) {
        return runPg0("psql", "--name", this.name, ...args);
    }
    /** Run psql with the given arguments (synchronous). */
    psqlSync(...args) {
        return runPg0Sync("psql", "--name", this.name, ...args);
    }
    /** Execute a SQL command and return the output. */
    async execute(sql) {
        return (await this.psql("-c", sql)).stdout;
    }
    /** Execute a SQL command and return the output (synchronous). */
    executeSync(sql) {
        return this.psqlSync("-c", sql);
    }
}
exports.Pg0 = Pg0;
/** List all pg0 instances. */
async function listInstances() {
    const { stdout } = await runPg0("list", "-o", "json");
    return JSON.parse(stdout);
}
/** List all pg0 instances (synchronous). */
function listInstancesSync() {
    return JSON.parse(runPg0Sync("list", "-o", "json"));
}
/** List available PostgreSQL extensions. */
async function listExtensions() {
    const { stdout } = await runPg0("list-extensions");
    return stdout.trim().split("\n").filter(line => line.trim());
}
/** List available PostgreSQL extensions (synchronous). */
function listExtensionsSync() {
    const stdout = runPg0Sync("list-extensions");
    return stdout.trim().split("\n").filter(line => line.trim());
}
/** Start a PostgreSQL instance (convenience function). */
async function start(options = {}) {
    return new Pg0(options).start();
}
/** Start a PostgreSQL instance (synchronous convenience function). */
function startSync(options = {}) {
    return new Pg0(options).startSync();
}
/** Stop a PostgreSQL instance (convenience function). */
async function stop(name = "default") {
    try {
        await runPg0("stop", "--name", name);
    }
    catch (e) {
        if (!(e instanceof Pg0NotRunningError))
            throw e;
    }
}
/** Stop a PostgreSQL instance (synchronous convenience function). */
function stopSync(name = "default") {
    try {
        runPg0Sync("stop", "--name", name);
    }
    catch (e) {
        if (!(e instanceof Pg0NotRunningError))
            throw e;
    }
}
/** Drop a PostgreSQL instance (stop if running, delete all data). */
async function drop(name = "default", force = true) {
    const args = ["drop", "--name", name];
    if (force)
        args.push("--force");
    try {
        await runPg0(...args);
    }
    catch {
        // Ignore errors
    }
}
/** Drop a PostgreSQL instance (synchronous convenience function). */
function dropSync(name = "default", force = true) {
    const args = ["drop", "--name", name];
    if (force)
        args.push("--force");
    try {
        runPg0Sync(...args);
    }
    catch {
        // Ignore errors
    }
}
/** Get information about a PostgreSQL instance (convenience function). */
async function info(name = "default") {
    const { stdout } = await runPg0("info", "--name", name, "-o", "json");
    return JSON.parse(stdout);
}
/** Get information about a PostgreSQL instance (synchronous convenience function). */
function infoSync(name = "default") {
    return JSON.parse(runPg0Sync("info", "--name", name, "-o", "json"));
}
// Keep PostgreSQL as alias for backwards compatibility
exports.PostgreSQL = Pg0;
exports.default = Pg0;
