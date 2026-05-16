# AGENTS.md - PartBoot Development Guide

## Build / Lint / Test

```powershell
cargo build --release                    # Release binary
cargo run -- <command>                   # Run CLI (e.g. cargo run -- start)
cargo test                               # All tests
cargo test <test_name>                   # Single test (substring match)
cargo test boot_entry::tests::            # All tests in a module
cargo fmt --check                        # Format check
cargo fmt                                # Auto-format
cargo clippy --all-targets -- -D warnings # Lint (treat warnings as errors)
```

If using `x86_64-pc-windows-gnu` toolchain, ensure `dlltool.exe` is on `PATH` (e.g. `C:\msys64\ucrt64\bin`).

**Before committing:** run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, then `cargo test`.

## Project Layout

```
src/
  boot_entry.rs  UEFI boot entry management (Windows-only)
  cache.rs       EFI asset lookup, download, checksums
  extract.rs     ISO boot-file extraction via 7-Zip
  grub.rs        GRUB menu generation
  iso.rs         ISO discovery and family classification
  layout.rs      PartBoot directory layout helpers
  main.rs        CLI parsing, workflows, ESP install, TUI
  profile.rs     Per-ISO boot profile loading and repair
  spinner.rs     Terminal progress UI
```

## Code Style

### Imports
- Group `use crate::` imports together, then `use std::` imports.
- Sort alphabetically within groups.
- Use `use crate::module::{TypeA, TypeB};` for multi-imports from one module.

### Formatting
- 4-space indentation (no tabs). `cargo fmt` enforces this.
- Max line length: 100 chars (rustfmt default). Let rustfmt handle line breaks.

### Naming
- `snake_case` for functions, variables, modules, fields.
- `PascalCase` for structs, enums, traits.
- `SCREAMING_SNAKE_CASE` for constants.
- Boolean variables: prefer descriptive names (`dry_run`, `add_first`, `partboot_only`).

### Types
- Derive `#[derive(Debug, Clone, PartialEq, Eq)]` on all structs and enums.
- Use `Option<T>` for nullable fields.
- Use `Result<T, String>` for error returns (not custom error types).
- Use `&Path` / `&PathBuf` for file paths, not `&str`.

### Error Handling
- Return `Result<T, String>` with human-readable error messages.
- Use `?` operator for propagation.
- Format errors with context: `Err(format!("failed to run bcdedit: {error}"))`.
- Use `.map_err(|e| e.to_string())?` for standard library errors.
- Platform-specific functions use `#[cfg(windows)]` / `#[cfg(not(windows))]` with a stub returning `Err("...supported on Windows UEFI only")`.

### Platform-Specific Code
```rust
pub fn some_function() -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (arg1, arg2); // suppress unused warnings
        Err("supported on Windows UEFI only".to_string())
    }
    #[cfg(windows)]
    {
        // real implementation
    }
}
```

### Tests
- Place tests in `mod tests { ... }` at the bottom of each file.
- Use `#[test]` attribute on each test function.
- Name tests descriptively: `parse_copied_identifier_extracts_guid`.
- Test both success and failure cases.
- Use `assert_eq!`, `assert!`, `assert!(result.is_err())`.

### CLI Commands
- Commands use `clap` with `#[derive(Subcommand)]` in `main.rs`.
- Top-level `enum Command` for main commands, `enum BootEntryCommand` for subcommands.
- `Cli::parse_from()` in tests for parsing verification.
- No args defaults to `Command::Start` (interactive TUI).
- Use `--json` flag for machine-readable output.
- Use `--dry-run` / `--dry-run-install` for non-destructive validation.
- Print `[ok]` prefix for success messages.

### JSON Output
- When `--json` is set, output a single JSON object per command.
- Use a `json_escape()` helper for string values.
- Do not mix human-readable output with JSON output.

### Spinner / Progress UI
- Use `spinner.rs` for long-running operations (extraction, EFI staging).
- Call `spinner.finish(&message)` on success, `spinner.finish_error(&message)` on failure.
- Keep spinner messages concise and action-oriented.

### Safety Rules
- PartBoot touches boot files and EFI partitions.
- Destructive operations must have a `--dry-run` path.
- Back up BCD before any firmware modification.
- Validate inputs before making changes.
- Provide clear error messages with recovery guidance.
- Never guess or auto-select partitions without user confirmation.

### Documentation Conventions
- Keep `README.md` concise and user-facing.
- Put command details and troubleshooting in `docs/usage.md`.
- Put build, release, and implementation details in `DEVELOPMENT.md`.
- Put design records under `docs/architecture/`.
- Put planning notes under `docs/plans/`.

### Runtime Data Layout
Generated runtime data lives under the selected PartBoot root:
```
partboot/
  isos/        ISO images
  cache/       cached EFI assets
  extracted/   extracted boot files
  profiles/    per-ISO boot profiles
  efi/         staged EFI files
  generated/   generated GRUB configuration
```

### Environment Variables
- `PARTBOOT_7Z_PATH` - Full path to `7z.exe` when not on `PATH`.
- `PARTBOOT_EFI_ASSETS` - Directory containing bundled EFI assets. Defaults to `assets\efi`.

### Release Packaging
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu
```
The packaging script validates EFI provenance, computes checksums, and produces release ZIPs.
