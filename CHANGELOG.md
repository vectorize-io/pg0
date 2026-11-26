# Changelog

All notable changes to pg0 are documented in this file.

## [0.18.0] - 2026-11-26

### Security

- Bundle PostgreSQL 18.7.0 (was 18.1.0). This fixes CVE-2026-116331 and includes the security fixes from 18.2 through 18.6 (#53). Existing instances move to 18.7 on their next `pg0 start`. A minor upgrade needs no dump and restore.

### Added

- `pg0 start` reads the password from `PG0_PASSWORD` when `--password` is not given, so it stays out of the process list and shell history (#60).

### Fixed

- Alpine (musl): pgvector now works. The musl binaries bundled the glibc build of pgvector, so `CREATE EXTENSION vector` failed (#54).
- Windows: `pg0 start` no longer waits about 4 seconds when the server listens on 127.0.0.1 only (#56).
- After an upgrade, pg0 uses the newest PostgreSQL version folder in the installation directory instead of whichever one the OS lists first.

## [0.17.0] - 2026-11-17

### Added

- `pg0 ui`: a read-only web UI for all your pg0 instances, served by pg0 itself. Browse tables, run read-only SQL, and see a per-database overview (structure diagram, sizes, vacuum and index stats, live activity). It listens on `0.0.0.0:5050` by default (`--host`, `--port` to change) and is protected by a per-run token. All access goes through a temporary read-only role inside read-only transactions.

## [0.16.1] - 2026-11-04

### Fixed

- macOS: PostgreSQL now starts on Macs without Homebrew. OpenSSL is bundled instead of being loaded from `/opt/homebrew`, and installs left broken by earlier versions are repaired on the next `pg0 start` (#42, #51).
- Windows: `pg0 start` no longer fails with "stream did not contain valid UTF-8" on Chinese and other non-UTF-8 locales, and `pg0 logs` no longer fails on such logs (#46).

## [0.16.0] - 2026-11-04

### Added

- Optional `PG0_EXECUTABLE` (Python SDK), `PG0_INSTALLATION_DIR`, and `PG0_HOME` environment variables to choose where the pg0 executable, the extracted PostgreSQL files, and instance state live, e.g. for Windows machines with application-control policies (#43).

### Fixed

- `Pg0.start()` in the Python SDK no longer hangs forever on Linux and macOS when PostgreSQL keeps pg0's output handles open (#45).
- `pg0 info` and `pg0 list` no longer report a live instance as stopped on slow Windows machines: the health check now waits up to 10s to connect, and prints psql's error to stderr when it fails (#41).
- Linux glibc builds: libxml2 is now pinned to `2.9.13+dfsg-1ubuntu0.14`, after Ubuntu removed the previous build from its archive. Bundled Ubuntu packages now fall back to Launchpad when the archive no longer has them.

### Security

- Updated `Cargo.lock` dependencies to clear 24 advisories (quinn-proto, rustls, rustls-webpki, tokio, tar, and others) (#44).

## [0.15.2] - 2026-09-14

### Fixed

- `start`, `stop`, and `drop` no longer trust a saved pid that the OS reused after a reboot; a pid only counts as running if it matches `postmaster.pid` and is a live postgres process. This fixes a permanent "Instance already running" refusal and prevents `stop` from signalling an unrelated process (#37).
- `instance.json` without a `pid` (or with `null` / `0`) no longer breaks `start` (#36).
- New Windows clusters are initialized with `--locale=C`, so `initdb` no longer fails on localized locale names such as `Turkish_Türkiye.1252` (#35). Existing data directories are unchanged.

## [0.15.1] - 2026-07-31

### Fixed

- Suppress the Windows console window when the Python SDK launches the pg0 CLI from GUI applications.

## [0.15.0] - 2026-07-29

### Added

- Active database health checks for `pg0 info`, `pg0 list`, and SDK status reporting.

### Changed

- Upgraded bundled pgvector to 0.8.5.
- Rebuilt Linux pgvector artifacts against a GLIBC 2.35 baseline for compatibility with supported GNU/Linux hosts.
- Removed the Node.js SDK and npm release support.

### Fixed

- Windows startup for existing instances no longer depends on the blocking `tasklist` command.

## [0.14.2] - 2026-05-28

### Fixed

- Wait for PostgreSQL to fully shut down before returning from `pg0 stop`.

## [0.14.1] - 2026-05-08

### Fixed

- Make existing-instance startup independent of localized duplicate-database error messages.

## [0.14.0] - 2026-05-05

### Changed

- Bundle libxml2 and ICU runtime libraries for broader Linux compatibility.

## [0.13.0] - 2026-04-30

### Changed

- Synchronize the committed Cargo lockfile as part of releases for reproducible builds.
