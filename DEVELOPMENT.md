# Developer Guide

This guide covers local development, automation, release packaging, and implementation details for PartBoot.

## Prerequisites

- Rust 1.95 or newer.
- Git.
- Microsoft C++ Build Tools or Visual Studio with the C++ workload when using the default Windows MSVC Rust toolchain.
- 7-Zip available in `PATH`, or `PARTBOOT_7Z_PATH` set to the full path of `7z.exe`.
- PowerShell 7+ for release scripts.
- A disposable test partition for boot workflow testing.

## Build and Test

Build the release binary:

```powershell
cargo build --release
```

Run the test suite:

```powershell
cargo test
```

Run the CLI during development:

```powershell
cargo run -- start
cargo run -- scan --root H:\partboot
cargo run -- doctor --root H:\partboot
```

Before opening a pull request, run:

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Project Layout

```text
src/
  cache.rs     EFI asset lookup, download, and checksum handling
  extract.rs   ISO boot-file extraction and extracted-image validation
  grub.rs      GRUB menu generation
  iso.rs       ISO discovery and distribution-family detection
  layout.rs    PartBoot directory layout helpers
  main.rs      CLI parsing, workflows, ESP installation, and TUI
  profile.rs   Per-ISO boot profile loading and repair
  spinner.rs   Terminal progress UI
```

Generated runtime data is stored under the selected PartBoot root:

```text
partboot/
  isos/        ISO images
  cache/       cached EFI assets
  extracted/   extracted boot files
  profiles/    per-ISO boot profiles
  efi/         staged EFI files
  generated/   generated GRUB configuration
```

## Automation

Commands that support `--json` produce machine-readable output:

```powershell
partboot scan --root H:\partboot --json
partboot generate-menu --root H:\partboot --partition-uuid 9412B8E612B8CF0C --partition-label PARTBOOT --json
partboot doctor --root H:\partboot --esp S:\ --json
partboot guided-test-flow --root H:\partboot --esp S:\ --partition-uuid 9412B8E612B8CF0C --json
```

Environment variables:

| Variable | Purpose |
| --- | --- |
| `PARTBOOT_7Z_PATH` | Full path to `7z.exe` when it is not on `PATH`. |
| `PARTBOOT_EFI_ASSETS` | Directory containing bundled EFI assets. Defaults to `assets\efi`. |
| `PARTBOOT_EFI_RELEASE_BASE` | GitHub API base URL for EFI asset downloads. |
| `PARTBOOT_EFI_RELEASE_TAG` | Release tag used when downloading EFI assets. |

## Release Packaging

Build a release bundle:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu
```

The packaging script:

- Validates EFI provenance in `docs/release-efi-provenance.md`.
- Rebuilds `grubx64.efi` with `grub-mkstandalone` when GRUB tooling is available.
- Computes EFI checksums.
- Produces the release ZIP bundle.

Refresh EFI checksums after replacing EFI assets:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -RefreshChecksums
```

For local packaging tests only:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -SkipStandaloneGrubBuild -SkipProvenanceCheck
```

After publishing a GitHub release, verify required assets:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-assets.ps1 -Tag v0.2.3
```

Expected release assets:

- `partboot.exe`
- `bootx64.efi`
- `grubx64.efi`
- `checksums.txt`
- `partboot-VERSION-x86_64-pc-windows-gnu.zip`

## WinGet Submission

The `.github/workflows/winget-submit.yml` workflow submits a package manifest to Microsoft WinGet after a stable GitHub release is published.

Required repository secret:

| Secret | Purpose |
| --- | --- |
| `WINGET_TOKEN` | GitHub personal access token with `public_repo` scope. |

The workflow skips prereleases and stale tags. It also supports manual backfill through workflow dispatch.

## Implementation Notes

### ISO Extraction

PartBoot uses 7-Zip to extract boot files:

| Family | Extracted paths |
| --- | --- |
| Ubuntu / Debian | `casper/{vmlinuz,initrd,filesystem.squashfs}` |
| Arch | `arch/{boot,x86_64}` |
| Fedora | `isolinux/{vmlinuz,initrd.img}` |

Extracted files are cached under `partboot\extracted\<iso-id>\`.

### Boot Modes

- `iso_toram`: copies the ISO into RAM before booting.
- `iso_scan`: boots from the ISO stored on disk.
- `extracted`: boots kernel, initrd, and filesystem files extracted to disk.

The default profile uses `iso_toram` with hidden fallback entries unless `visible_fallback = true` is set in the profile.

### Profiles

Profiles are TOML files stored in `partboot\profiles\<iso-id>.toml`:

```toml
[boot]
preferred_mode = "iso_toram"
fallback_mode = "iso_scan"
visible_fallback = false
```

Profiles are created automatically and repaired when stale or incomplete.

### EFI Assets

On startup, PartBoot resolves EFI binaries in this order:

1. Bundled assets in `assets\efi` or `PARTBOOT_EFI_ASSETS`.
2. Cached assets in the PartBoot root.
3. Matching GitHub Release assets.

Checksums are validated before cached EFI assets are used.

## Planning Documents

- [MVP baseline](docs/plans/2026-05-08-partboot-mvp.md)
- [Ubuntu shutdown fix](docs/plans/2026-05-07-shutdown-loop-fix.md)
- [Phase 4 integration](docs/plans/2026-05-12-phase-4-platform-integration.md)
- [Menu profile fixes](docs/plans/2026-05-08-clean-menu-profiles.md)
