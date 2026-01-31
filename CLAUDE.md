# Claude Code Development Guidelines

## Platform Support Requirements

### Mandatory Components for All Platforms

**CRITICAL:** All supported platforms MUST include the following components:

1. **PostgreSQL** - Core database (required)
2. **pgvector** - Vector similarity search extension (required)
3. **pgbouncer** - Connection pooling (required)

### Platform Support Policy

- **Never suggest or ship** platform support without all three components
- If a component is missing for a platform (e.g., pgvector not compiled for that platform):
  - DO NOT suggest shipping without it
  - DO NOT present it as an optional component
  - The platform is NOT supported until all components are available

### Adding New Platform Support

When adding support for a new platform (e.g., `x86_64-apple-darwin`):

1. Ensure PostgreSQL binaries exist for the platform
2. Ensure pgvector compiled binaries exist for the platform
3. Ensure pgbouncer compiled binaries exist for the platform
4. Add the platform to CI/CD workflows
5. Test the complete build with all three components

### Current Platform Support

All platforms in `build.rs` and `.github/workflows/release-cli.yml` must have:
- PostgreSQL from `theseus-rs/postgresql-binaries`
- pgvector from `nicoloboschi/pgvector_compiled`
- pgbouncer from `nicoloboschi/pgbouncer_compiled`

## Build Requirements

- **All builds must succeed with all three components bundled**
- **Build MUST FAIL if any component is missing** for supported platforms
- No graceful fallbacks - missing components = build failure
- This ensures platforms are only released when fully functional
