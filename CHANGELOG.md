# Changelog

All notable changes to pg0 are documented in this file.

## [Unreleased]

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
