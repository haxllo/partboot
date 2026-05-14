# PartBoot Development & Advanced Usage

This document covers developer workflows, release procedures, and advanced command-line usage.

## Building from Source

### Requirements

- Rust (stable-x86_64-pc-windows-gnu toolchain)
- 7-Zip installed
- PowerShell 7+ (for release scripts)

### Build Commands

Compile the binary:

```powershell
cargo +stable-x86_64-pc-windows-gnu build --release
```

Run tests:

```powershell
cargo +stable-x86_64-pc-windows-gnu test
```

Run with a subcommand:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- start
cargo run -- scan --root <ROOT_PATH>
cargo run -- extract --root <ROOT_PATH> --iso <ISO_NAME>
```

## All Commands

### init

Initialize a PartBoot directory structure.

```
partboot init --root <ROOT_PATH>
```

### scan

Scan the partition for ISO files and build a profile index.

```
partboot scan --root <ROOT_PATH>
partboot scan --root <ROOT_PATH> --json
```

### extract

Extract boot files from an ISO (requires 7z in PATH or `PARTBOOT_7Z_PATH`).

```
partboot extract --root <ROOT_PATH> --iso <ISO_NAME_OR_PATH>
```

### volume-id

Get the NTFS serial or partition UUID for menu generation.

```
partboot volume-id --drive <DRIVE_LETTER:>
```

For NTFS, run from an elevated terminal to get the full serial. Short serials (e.g., `12B8CF0C`) are rejected; use the full UUID (e.g., `9412B8E612B8CF0C`).

### generate-menu

Generate a GRUB menu configuration for the selected partition.

```
partboot generate-menu --root <ROOT_PATH> --partition-uuid <UUID> --partition-label <LABEL>
partboot generate-menu --root <ROOT_PATH> --partition-uuid <UUID> --partition-label <LABEL> --json
partboot generate-menu --root <ROOT_PATH> --partition-uuid <UUID> --partition-label <LABEL> --include-diagnostics
```

### stage-efi

Prepare EFI files for installation (does not write to real EFI partition).

```
partboot stage-efi --root <ROOT_PATH> --grub-x64 <PATH_TO_GRUBX64.EFI> --boot-x64 <PATH_TO_BOOTX64.EFI>
```

### install-esp

Copy staged EFI files to a test or real EFI partition. Requires `--force` or `--dry-run`.

```
partboot install-esp --root <ROOT_PATH> --esp <ESP_PATH> --force
partboot install-esp --root <ROOT_PATH> --esp <ESP_PATH> --dry-run
```

### install-fallback

Copy the loader to the UEFI fallback boot path (`EFI\Boot\bootx64.efi`).

```
partboot install-fallback --root <ROOT_PATH> --esp <ESP_PATH> --force
```

### boot-instructions

Print the manual boot path for firmware menus.

```
partboot boot-instructions --esp <ESP_PATH>
```

### doctor

Validate EFI files and diagnose common issues.

```
partboot doctor --root <ROOT_PATH> --esp <ESP_PATH>
partboot doctor --root <ROOT_PATH> --esp <ESP_PATH> --json
```

### guided-test-flow

Automated workflow: scan, extract, generate menu, and stage EFI (without installing).

```
partboot guided-test-flow --root <ROOT_PATH> --esp <ESP_PATH> --partition-uuid <UUID> --partition-label <LABEL>
```

### start

Interactive TUI wizard (recommended for most users).

```
partboot start
```

This command:
- Auto-detects partitions
- Prompts for partition selection (Up/Down arrows, `j`/`k`, number jump)
- Auto-imports ISOs if `isos/` is empty
- Extracts supported Linux ISOs
- Generates and displays the GRUB menu
- Shows installation instructions

## Environment Variables

### Path Overrides

- `PARTBOOT_7Z_PATH` — Path to `7z.exe` (if not in PATH)
- `PARTBOOT_EFI_ASSETS` — Path to bundled EFI assets directory (default: `assets\efi`)

### Release/Download Overrides

- `PARTBOOT_EFI_RELEASE_BASE` — GitHub API base URL (default: `https://api.github.com/repos/haxllo/partboot`)
- `PARTBOOT_EFI_RELEASE_TAG` — Release tag to download EFI binaries from (default: current app version)

Example: Use a custom GitHub fork:

```powershell
$env:PARTBOOT_EFI_RELEASE_BASE = "https://api.github.com/repos/myusername/partboot"
partboot start
```

## Release & Packaging

### Package a Release

Build a release bundle with bundled EFI assets and checksums:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu
```

This:
- Validates provenance documentation in `docs/release-efi-provenance.md`
- Rebuilds `grubx64.efi` using `grub-mkstandalone` (requires GRUB tools)
- Computes checksums for EFI binaries
- Creates a release ZIP bundle

If EFI binaries were replaced, regenerate checksums:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -RefreshChecksums
```

Bypass standalone GRUB rebuild (local testing only):

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -SkipStandaloneGrubBuild -SkipProvenanceCheck
```

### Verify Release Assets

After publishing a GitHub release, verify all required assets are present:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\check-release-assets.ps1 -Tag v0.2.3
```

Required assets:
- `partboot.exe`
- `bootx64.efi`
- `grubx64.efi`
- `checksums.txt`
- `partboot-VERSION-x86_64-pc-windows-gnu.zip`

### WinGet Submission

The `winget-submit` workflow (`.github/workflows/winget-submit.yml`) automatically submits a package manifest to Microsoft WinGet after a stable GitHub release is published.

**Required repository secret:**

- `WINGET_TOKEN` — GitHub personal access token with `public_repo` scope

The workflow:
- Skips prereleases (only stable releases)
- Skips stale tags (only the latest release)
- Supports manual backfill via workflow dispatch

To add the secret:
1. Go to repository Settings → Secrets and variables → Actions
2. Click "New repository secret"
3. Name: `WINGET_TOKEN`, Value: your GitHub PAT
4. Click "Add secret"

Create a PAT at: https://github.com/settings/tokens/new

## JSON Output for Automation

Commands that support `--json` flag output structured results:

```powershell
partboot scan --root H:\partboot --json
partboot generate-menu --root H:\partboot --partition-uuid ABC123 --partition-label MYPART --json
partboot doctor --root H:\partboot --esp E:\ --json
partboot guided-test-flow --root H:\partboot --esp E:\ --partition-uuid ABC123 --partition-label MYPART --json
```

Use this for scripting, CI/CD pipelines, or integration with other tools.

## Testing Partition Recommendation

For development and testing:

1. Create a **separate disposable NTFS partition** (16-64 GB)
2. Do not test on partitions containing personal data or system files
3. Test ISOs from different families (Ubuntu, Debian, Arch, Fedora) to verify multi-distro support
4. Test both ISO boot (fast iteration) and extracted boot (for RAM-constrained environments)

## Implementation Plans

- **MVP baseline**: `docs/plans/2026-05-08-partboot-mvp.md`
- **Ubuntu shutdown fix**: `docs/plans/2026-05-07-shutdown-loop-fix.md`
- **Phase 4 integration**: `docs/plans/2026-05-12-phase-4-platform-integration.md`
- **Menu profile fixes**: `docs/plans/2026-05-08-clean-menu-profiles.md`

## Technical Notes

### ISO Extraction

The `extract` command uses 7z to pull boot files:
- **Ubuntu/Debian**: `casper/{vmlinuz,initrd,filesystem.squashfs}`
- **Arch**: `arch/{boot,x86_64}`
- **Fedora**: `isolinux/{vmlinuz,initrd.img}`

Extracted files are cached in `partboot/extracted/<iso-id>/` to avoid re-extracting on subsequent runs.

### Boot Modes

- **ISO toram**: Copies the entire ISO into RAM (requires sufficient RAM, clean shutdown)
- **ISO scan**: Boots from the ISO on disk (slower, but lower RAM requirement)
- **Extracted**: Boots kernel + initrd + filesystem from disk (fastest, lowest RAM)

The default mode is ISO toram. Fallback modes are generated but hidden from the menu unless `visible_fallback=true` in the ISO profile.

### Profile System

Per-ISO boot configurations are stored in `partboot/profiles/<iso-id>.toml`:

```toml
[boot]
preferred_mode = "iso_toram"
fallback_mode = "iso_scan"
visible_fallback = false
```

Profiles are auto-created with sensible defaults and can be customized by users. Stale profiles are auto-repaired on the next run.

### EFI Asset Caching

On first run, `partboot start` caches EFI binaries:
1. Check bundled assets in `assets/efi`
2. If missing, download from GitHub Releases (matches app version)
3. Verify checksums before caching
4. Store in `partboot/cache`

This allows offline-capable releases and fallback to public GitHub if bundled assets are unavailable.

## Contributing

This project uses:
- **Rust** with `cargo` for building and testing
- **crossterm** for terminal UI
- **serde_json** for profile serialization
- **7-Zip** for ISO extraction

All code should be tested before submission. Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu test
```

Code follows standard Rust conventions (checked by `rustfmt` and `clippy`).
