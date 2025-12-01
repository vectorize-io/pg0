import { execFile, execFileSync } from "child_process";
import { createWriteStream, chmodSync, existsSync, mkdirSync, renameSync, unlinkSync } from "fs";
import { get } from "https";
import { homedir, platform, arch } from "os";
import { join } from "path";
import { promisify } from "util";

const execFileAsync = promisify(execFile);

const PG0_REPO = "vectorize-io/pg0";

export class Pg0Error extends Error {
  constructor(message: string) {
    super(message);
    this.name = "Pg0Error";
  }
}

export class Pg0NotFoundError extends Pg0Error {
  constructor(message: string = "pg0 binary not found") {
    super(message);
    this.name = "Pg0NotFoundError";
  }
}

export class Pg0NotRunningError extends Pg0Error {
  constructor(message: string = "PostgreSQL instance is not running") {
    super(message);
    this.name = "Pg0NotRunningError";
  }
}

export class Pg0AlreadyRunningError extends Pg0Error {
  constructor(message: string = "PostgreSQL instance is already running") {
    super(message);
    this.name = "Pg0AlreadyRunningError";
  }
}

export interface InstanceInfo {
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

export interface Pg0Options {
  /** Instance name (allows multiple instances) */
  name?: string;
  /** Port to listen on */
  port?: number;
  /** Database username */
  username?: string;
  /** Database password */
  password?: string;
  /** Database name */
  database?: string;
  /** Custom data directory */
  dataDir?: string;
  /** PostgreSQL configuration options */
  config?: Record<string, string>;
}

function getInstallDir(): string {
  if (process.platform === "win32") {
    const base = process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local");
    return join(base, "pg0", "bin");
  }
  return join(homedir(), ".local", "bin");
}

function getPlatform(): string {
  const os = platform();
  const cpu = arch();

  if (os === "darwin") {
    return "darwin-aarch64"; // Intel Macs use Rosetta
  } else if (os === "linux") {
    if (cpu === "x64") {
      return "linux-x86_64";
    }
    throw new Pg0NotFoundError(`Unsupported Linux architecture: ${cpu}`);
  } else if (os === "win32") {
    return "windows-x86_64";
  }
  throw new Pg0NotFoundError(`Unsupported platform: ${os}`);
}

async function getLatestVersion(): Promise<string> {
  return new Promise((resolve, reject) => {
    const url = `https://api.github.com/repos/${PG0_REPO}/releases/latest`;
    get(url, { headers: { "User-Agent": "pg0-node" } }, (res) => {
      if (res.statusCode === 302 || res.statusCode === 301) {
        get(res.headers.location!, { headers: { "User-Agent": "pg0-node" } }, handleResponse);
        return;
      }
      handleResponse(res);

      function handleResponse(response: typeof res) {
        let data = "";
        response.on("data", (chunk) => (data += chunk));
        response.on("end", () => {
          try {
            const json = JSON.parse(data);
            resolve(json.tag_name);
          } catch (e) {
            reject(new Pg0NotFoundError(`Failed to parse version: ${e}`));
          }
        });
      }
    }).on("error", (e) => reject(new Pg0NotFoundError(`Failed to fetch version: ${e}`)));
  });
}

function downloadFile(url: string, dest: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const file = createWriteStream(dest);

    function doDownload(downloadUrl: string) {
      get(downloadUrl, { headers: { "User-Agent": "pg0-node" } }, (res) => {
        if (res.statusCode === 302 || res.statusCode === 301) {
          doDownload(res.headers.location!);
          return;
        }
        if (res.statusCode !== 200) {
          reject(new Pg0NotFoundError(`Download failed: ${res.statusCode}`));
          return;
        }
        res.pipe(file);
        file.on("finish", () => {
          file.close();
          resolve();
        });
      }).on("error", (e) => {
        unlinkSync(dest);
        reject(new Pg0NotFoundError(`Download failed: ${e}`));
      });
    }

    doDownload(url);
  });
}

/**
 * Install the pg0 binary.
 * @param version Version to install (default: latest)
 * @param force Force reinstall even if already installed
 * @returns Path to installed binary
 */
export async function install(version?: string, force: boolean = false): Promise<string> {
  const installDir = getInstallDir();
  const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
  const binaryPath = join(installDir, binaryName);

  if (existsSync(binaryPath) && !force) {
    return binaryPath;
  }

  if (!version) {
    version = await getLatestVersion();
  }

  const plat = getPlatform();
  const ext = process.platform === "win32" ? ".exe" : "";
  const filename = `pg0-${plat}${ext}`;
  const url = `https://github.com/${PG0_REPO}/releases/download/${version}/${filename}`;

  console.log(`Installing pg0 ${version}...`);

  mkdirSync(installDir, { recursive: true });

  const tmpPath = join(installDir, `pg0.tmp${ext}`);
  await downloadFile(url, tmpPath);

  renameSync(tmpPath, binaryPath);

  if (process.platform !== "win32") {
    chmodSync(binaryPath, 0o755);
  }

  console.log(`Installed pg0 to ${binaryPath}`);
  return binaryPath;
}

/**
 * Install the pg0 binary synchronously.
 */
export function installSync(version?: string, force: boolean = false): string {
  const installDir = getInstallDir();
  const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
  const binaryPath = join(installDir, binaryName);

  if (existsSync(binaryPath) && !force) {
    return binaryPath;
  }

  // For sync, we use execSync to call curl
  const { execSync } = require("child_process");

  if (!version) {
    try {
      const result = execSync(
        `curl -sL https://api.github.com/repos/${PG0_REPO}/releases/latest`,
        { encoding: "utf-8" }
      );
      version = JSON.parse(result).tag_name;
    } catch (e) {
      throw new Pg0NotFoundError(`Failed to fetch version: ${e}`);
    }
  }

  const plat = getPlatform();
  const ext = process.platform === "win32" ? ".exe" : "";
  const filename = `pg0-${plat}${ext}`;
  const url = `https://github.com/${PG0_REPO}/releases/download/${version}/${filename}`;

  console.log(`Installing pg0 ${version}...`);

  mkdirSync(installDir, { recursive: true });

  try {
    execSync(`curl -fsSL "${url}" -o "${binaryPath}"`, { encoding: "utf-8" });
    if (process.platform !== "win32") {
      chmodSync(binaryPath, 0o755);
    }
    console.log(`Installed pg0 to ${binaryPath}`);
    return binaryPath;
  } catch (e) {
    throw new Pg0NotFoundError(`Failed to install pg0: ${e}`);
  }
}

function findPg0Sync(): string {
  // Check PATH
  const { execSync } = require("child_process");
  try {
    const result = execSync("which pg0 2>/dev/null || where pg0 2>nul", { encoding: "utf-8" });
    if (result.trim()) return result.trim().split("\n")[0];
  } catch {
    // Not in PATH
  }

  // Check install location
  const installDir = getInstallDir();
  const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
  const binaryPath = join(installDir, binaryName);

  if (existsSync(binaryPath)) {
    return binaryPath;
  }

  // Auto-install
  return installSync();
}

async function findPg0(): Promise<string> {
  // Check install location first (faster than which)
  const installDir = getInstallDir();
  const binaryName = process.platform === "win32" ? "pg0.exe" : "pg0";
  const binaryPath = join(installDir, binaryName);

  if (existsSync(binaryPath)) {
    return binaryPath;
  }

  // Check PATH
  const { execSync } = require("child_process");
  try {
    const result = execSync("which pg0 2>/dev/null || where pg0 2>nul", { encoding: "utf-8" });
    if (result.trim()) return result.trim().split("\n")[0];
  } catch {
    // Not in PATH
  }

  // Auto-install
  return install();
}

async function runPg0(...args: string[]): Promise<{ stdout: string; stderr: string }> {
  const pg0Path = await findPg0();
  try {
    return await execFileAsync(pg0Path, args);
  } catch (error: any) {
    const stderr = error.stderr || error.message || "";
    if (stderr.toLowerCase().includes("already running")) {
      throw new Pg0AlreadyRunningError(stderr);
    } else if (
      stderr.toLowerCase().includes("no running instance") ||
      stderr.toLowerCase().includes("not running")
    ) {
      throw new Pg0NotRunningError(stderr);
    }
    throw new Pg0Error(stderr || "pg0 command failed");
  }
}

function runPg0Sync(...args: string[]): string {
  const pg0Path = findPg0Sync();
  try {
    return execFileSync(pg0Path, args, { encoding: "utf-8" });
  } catch (error: any) {
    const stderr = error.stderr || error.message || "";
    if (stderr.toLowerCase().includes("already running")) {
      throw new Pg0AlreadyRunningError(stderr);
    } else if (
      stderr.toLowerCase().includes("no running instance") ||
      stderr.toLowerCase().includes("not running")
    ) {
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
export class Pg0 {
  readonly name: string;
  readonly port: number;
  readonly username: string;
  readonly password: string;
  readonly database: string;
  readonly dataDir?: string;
  readonly config: Record<string, string>;

  constructor(options: Pg0Options = {}) {
    this.name = options.name ?? "default";
    this.port = options.port ?? 5432;
    this.username = options.username ?? "postgres";
    this.password = options.password ?? "postgres";
    this.database = options.database ?? "postgres";
    this.dataDir = options.dataDir;
    this.config = options.config ?? {};
  }

  /** Start the PostgreSQL instance. */
  async start(): Promise<InstanceInfo> {
    const args = this._buildStartArgs();
    await runPg0(...args);
    return this.info();
  }

  /** Start the PostgreSQL instance (synchronous). */
  startSync(): InstanceInfo {
    const args = this._buildStartArgs();
    runPg0Sync(...args);
    return this.infoSync();
  }

  private _buildStartArgs(): string[] {
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
  async stop(): Promise<void> {
    try {
      await runPg0("stop", "--name", this.name);
    } catch (e) {
      // Ignore "not running" errors
      if (!(e instanceof Pg0NotRunningError)) throw e;
    }
  }

  /** Stop the PostgreSQL instance (synchronous). */
  stopSync(): void {
    try {
      runPg0Sync("stop", "--name", this.name);
    } catch (e) {
      // Ignore "not running" errors
      if (!(e instanceof Pg0NotRunningError)) throw e;
    }
  }

  /** Drop the PostgreSQL instance (stop if running, delete all data). */
  async drop(force: boolean = true): Promise<void> {
    const args = ["drop", "--name", this.name];
    if (force) args.push("--force");
    try {
      await runPg0(...args);
    } catch {
      // Ignore errors
    }
  }

  /** Drop the PostgreSQL instance (synchronous). */
  dropSync(force: boolean = true): void {
    const args = ["drop", "--name", this.name];
    if (force) args.push("--force");
    try {
      runPg0Sync(...args);
    } catch {
      // Ignore errors
    }
  }

  /** Get information about the PostgreSQL instance. */
  async info(): Promise<InstanceInfo> {
    const { stdout } = await runPg0("info", "--name", this.name, "-o", "json");
    return JSON.parse(stdout);
  }

  /** Get information about the PostgreSQL instance (synchronous). */
  infoSync(): InstanceInfo {
    const stdout = runPg0Sync("info", "--name", this.name, "-o", "json");
    return JSON.parse(stdout);
  }

  /** Get the connection URI if running. */
  async getUri(): Promise<string | undefined> {
    return (await this.info()).uri;
  }

  /** Get the connection URI if running (synchronous). */
  getUriSync(): string | undefined {
    return this.infoSync().uri;
  }

  /** Check if the instance is running. */
  async isRunning(): Promise<boolean> {
    return (await this.info()).running;
  }

  /** Check if the instance is running (synchronous). */
  isRunningSync(): boolean {
    return this.infoSync().running;
  }

  /** Run psql with the given arguments. */
  async psql(...args: string[]): Promise<{ stdout: string; stderr: string }> {
    return runPg0("psql", "--name", this.name, ...args);
  }

  /** Run psql with the given arguments (synchronous). */
  psqlSync(...args: string[]): string {
    return runPg0Sync("psql", "--name", this.name, ...args);
  }

  /** Execute a SQL command and return the output. */
  async execute(sql: string): Promise<string> {
    return (await this.psql("-c", sql)).stdout;
  }

  /** Execute a SQL command and return the output (synchronous). */
  executeSync(sql: string): string {
    return this.psqlSync("-c", sql);
  }
}

/** List all pg0 instances. */
export async function listInstances(): Promise<InstanceInfo[]> {
  const { stdout } = await runPg0("list", "-o", "json");
  return JSON.parse(stdout);
}

/** List all pg0 instances (synchronous). */
export function listInstancesSync(): InstanceInfo[] {
  return JSON.parse(runPg0Sync("list", "-o", "json"));
}

/** Start a PostgreSQL instance (convenience function). */
export async function start(options: Pg0Options = {}): Promise<InstanceInfo> {
  return new Pg0(options).start();
}

/** Start a PostgreSQL instance (synchronous convenience function). */
export function startSync(options: Pg0Options = {}): InstanceInfo {
  return new Pg0(options).startSync();
}

/** Stop a PostgreSQL instance (convenience function). */
export async function stop(name: string = "default"): Promise<void> {
  try {
    await runPg0("stop", "--name", name);
  } catch (e) {
    if (!(e instanceof Pg0NotRunningError)) throw e;
  }
}

/** Stop a PostgreSQL instance (synchronous convenience function). */
export function stopSync(name: string = "default"): void {
  try {
    runPg0Sync("stop", "--name", name);
  } catch (e) {
    if (!(e instanceof Pg0NotRunningError)) throw e;
  }
}

/** Drop a PostgreSQL instance (stop if running, delete all data). */
export async function drop(name: string = "default", force: boolean = true): Promise<void> {
  const args = ["drop", "--name", name];
  if (force) args.push("--force");
  try {
    await runPg0(...args);
  } catch {
    // Ignore errors
  }
}

/** Drop a PostgreSQL instance (synchronous convenience function). */
export function dropSync(name: string = "default", force: boolean = true): void {
  const args = ["drop", "--name", name];
  if (force) args.push("--force");
  try {
    runPg0Sync(...args);
  } catch {
    // Ignore errors
  }
}

/** Get information about a PostgreSQL instance (convenience function). */
export async function info(name: string = "default"): Promise<InstanceInfo> {
  const { stdout } = await runPg0("info", "--name", name, "-o", "json");
  return JSON.parse(stdout);
}

/** Get information about a PostgreSQL instance (synchronous convenience function). */
export function infoSync(name: string = "default"): InstanceInfo {
  return JSON.parse(runPg0Sync("info", "--name", name, "-o", "json"));
}

// Keep PostgreSQL as alias for backwards compatibility
export const PostgreSQL = Pg0;

export default Pg0;
