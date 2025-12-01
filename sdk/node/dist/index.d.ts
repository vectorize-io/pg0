export declare class Pg0Error extends Error {
    constructor(message: string);
}
export declare class Pg0NotFoundError extends Pg0Error {
    constructor();
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
export interface PostgreSQLOptions {
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
export declare class PostgreSQL {
    readonly name: string;
    readonly port: number;
    readonly username: string;
    readonly password: string;
    readonly database: string;
    readonly dataDir?: string;
    readonly config: Record<string, string>;
    constructor(options?: PostgreSQLOptions);
    /**
     * Start the PostgreSQL instance.
     * @returns Instance info with connection details
     */
    start(): Promise<InstanceInfo>;
    /**
     * Start the PostgreSQL instance (synchronous).
     * @returns Instance info with connection details
     */
    startSync(): InstanceInfo;
    /**
     * Stop the PostgreSQL instance.
     */
    stop(): Promise<void>;
    /**
     * Stop the PostgreSQL instance (synchronous).
     */
    stopSync(): void;
    /**
     * Get information about the PostgreSQL instance.
     */
    info(): Promise<InstanceInfo>;
    /**
     * Get information about the PostgreSQL instance (synchronous).
     */
    infoSync(): InstanceInfo;
    /**
     * Get the connection URI if running.
     */
    getUri(): Promise<string | undefined>;
    /**
     * Get the connection URI if running (synchronous).
     */
    getUriSync(): string | undefined;
    /**
     * Check if the instance is running.
     */
    isRunning(): Promise<boolean>;
    /**
     * Check if the instance is running (synchronous).
     */
    isRunningSync(): boolean;
    /**
     * Run psql with the given arguments.
     * @param args Arguments to pass to psql
     */
    psql(...args: string[]): Promise<{
        stdout: string;
        stderr: string;
    }>;
    /**
     * Run psql with the given arguments (synchronous).
     * @param args Arguments to pass to psql
     */
    psqlSync(...args: string[]): string;
    /**
     * Execute a SQL command and return the output.
     * @param sql SQL command to execute
     */
    execute(sql: string): Promise<string>;
    /**
     * Execute a SQL command and return the output (synchronous).
     * @param sql SQL command to execute
     */
    executeSync(sql: string): string;
}
/**
 * List all pg0 instances.
 */
export declare function listInstances(): Promise<InstanceInfo[]>;
/**
 * List all pg0 instances (synchronous).
 */
export declare function listInstancesSync(): InstanceInfo[];
/**
 * Start a PostgreSQL instance (convenience function).
 */
export declare function start(options?: PostgreSQLOptions): Promise<InstanceInfo>;
/**
 * Start a PostgreSQL instance (synchronous convenience function).
 */
export declare function startSync(options?: PostgreSQLOptions): InstanceInfo;
/**
 * Stop a PostgreSQL instance (convenience function).
 */
export declare function stop(name?: string): Promise<void>;
/**
 * Stop a PostgreSQL instance (synchronous convenience function).
 */
export declare function stopSync(name?: string): void;
/**
 * Get information about a PostgreSQL instance (convenience function).
 */
export declare function info(name?: string): Promise<InstanceInfo>;
/**
 * Get information about a PostgreSQL instance (synchronous convenience function).
 */
export declare function infoSync(name?: string): InstanceInfo;
export default PostgreSQL;
