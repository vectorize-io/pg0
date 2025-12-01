export declare class Pg0Error extends Error {
    constructor(message: string);
}
export declare class Pg0NotFoundError extends Pg0Error {
    constructor(message?: string);
}
export declare class Pg0NotRunningError extends Pg0Error {
    constructor(message?: string);
}
export declare class Pg0AlreadyRunningError extends Pg0Error {
    constructor(message?: string);
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
/**
 * Install the pg0 binary.
 * @param version Version to install (default: latest)
 * @param force Force reinstall even if already installed
 * @returns Path to installed binary
 */
export declare function install(version?: string, force?: boolean): Promise<string>;
/**
 * Install the pg0 binary synchronously.
 */
export declare function installSync(version?: string, force?: boolean): string;
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
export declare class Pg0 {
    readonly name: string;
    readonly port: number;
    readonly username: string;
    readonly password: string;
    readonly database: string;
    readonly dataDir?: string;
    readonly config: Record<string, string>;
    constructor(options?: Pg0Options);
    /** Start the PostgreSQL instance. */
    start(): Promise<InstanceInfo>;
    /** Start the PostgreSQL instance (synchronous). */
    startSync(): InstanceInfo;
    private _buildStartArgs;
    /** Stop the PostgreSQL instance. */
    stop(): Promise<void>;
    /** Stop the PostgreSQL instance (synchronous). */
    stopSync(): void;
    /** Drop the PostgreSQL instance (stop if running, delete all data). */
    drop(force?: boolean): Promise<void>;
    /** Drop the PostgreSQL instance (synchronous). */
    dropSync(force?: boolean): void;
    /** Get information about the PostgreSQL instance. */
    info(): Promise<InstanceInfo>;
    /** Get information about the PostgreSQL instance (synchronous). */
    infoSync(): InstanceInfo;
    /** Get the connection URI if running. */
    getUri(): Promise<string | undefined>;
    /** Get the connection URI if running (synchronous). */
    getUriSync(): string | undefined;
    /** Check if the instance is running. */
    isRunning(): Promise<boolean>;
    /** Check if the instance is running (synchronous). */
    isRunningSync(): boolean;
    /** Run psql with the given arguments. */
    psql(...args: string[]): Promise<{
        stdout: string;
        stderr: string;
    }>;
    /** Run psql with the given arguments (synchronous). */
    psqlSync(...args: string[]): string;
    /** Execute a SQL command and return the output. */
    execute(sql: string): Promise<string>;
    /** Execute a SQL command and return the output (synchronous). */
    executeSync(sql: string): string;
}
/** List all pg0 instances. */
export declare function listInstances(): Promise<InstanceInfo[]>;
/** List all pg0 instances (synchronous). */
export declare function listInstancesSync(): InstanceInfo[];
/** Start a PostgreSQL instance (convenience function). */
export declare function start(options?: Pg0Options): Promise<InstanceInfo>;
/** Start a PostgreSQL instance (synchronous convenience function). */
export declare function startSync(options?: Pg0Options): InstanceInfo;
/** Stop a PostgreSQL instance (convenience function). */
export declare function stop(name?: string): Promise<void>;
/** Stop a PostgreSQL instance (synchronous convenience function). */
export declare function stopSync(name?: string): void;
/** Drop a PostgreSQL instance (stop if running, delete all data). */
export declare function drop(name?: string, force?: boolean): Promise<void>;
/** Drop a PostgreSQL instance (synchronous convenience function). */
export declare function dropSync(name?: string, force?: boolean): void;
/** Get information about a PostgreSQL instance (convenience function). */
export declare function info(name?: string): Promise<InstanceInfo>;
/** Get information about a PostgreSQL instance (synchronous convenience function). */
export declare function infoSync(name?: string): InstanceInfo;
export declare const PostgreSQL: typeof Pg0;
export default Pg0;
