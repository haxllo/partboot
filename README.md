# PartBoot

> A disk-resident ISO boot manager for UEFI systems. Boot Linux ISOs from a local partition instead of creating USB drives for each image.

[![Crates.io](https://img.shields.io/crates/v/partboot?style=flat-square)](https://crates.io/crates/partboot)
[![GitHub release](https://img.shields.io/github/v/release/haxllo/partboot?style=flat-square)](https://github.com/haxllo/partboot/releases)
![Rust version](https://img.shields.io/badge/Rust-1.95+-orange?style=flat-square)
[![License](https://img.shields.io/badge/License-MIT-blue?style=flat-square)](LICENSE)

> [!WARNING]
> PartBoot is early-stage software. Test it on a disposable partition first. Do not point it at a partition containing personal data, an installed operating system, or recovery media.

## Features

- **Interactive setup** — guided workflow that scans partitions, imports ISOs, extracts boot files, generates a GRUB menu, and stages EFI binaries
- **Multi-distro support** — Ubuntu, Debian, Kali, Arch, Fedora, CachyOS, Omarchy, and other GRUB-compatible live images
- **GRUB menu generation** — automatic boot profiles per ISO with optional diagnostics entry
- **EFI staging** — staged install with `--dry-run` and `--force` safeguards
- **Boot entry management** — create, list, remove, and restore UEFI firmware boot entries with BCD backup and rollback
- **Health checks** — built-in `doctor` command validates the entire installation
- **JSON output** — machine-readable output for scripting and automation

## System Requirements

- Windows with UEFI firmware
- 7-Zip (for ISO extraction)
- A separate NTFS test partition (16–64 GB recommended)
- Secure Boot disabled (or provide your own trusted EFI signing flow)

## Installation

### WinGet

```powershell
winget install --id Haxllo.PartBoot --exact
```

### Cargo

```powershell
cargo install partboot
```

### GitHub Releases

Download `partboot.exe` from the [Releases](https://github.com/haxllo/partboot/releases) page.

## Quick Start

Run the interactive wizard:

```powershell
partboot
```

The wizard will:

1. Detect NTFS and FAT32 partitions
2. Guide you through root and ESP selection
3. Import ISO files into the PartBoot directory
4. Extract boot files from supported Linux ISOs
5. Generate a GRUB boot menu
6. Stage EFI binaries
7. Offer to create a persistent UEFI boot entry

After reviewing the generated files, install them to an EFI System Partition:

```powershell
partboot esp --root H:\partboot --esp S:\ --force
partboot fallback --root H:\partboot --esp S:\ --force
```

Reboot and select the PartBoot entry from the firmware boot menu.

## Commands

### Core

| Command | Description |
|---|---|
| `partboot` | Start the interactive guided workflow |
| `partboot init --root <path>` | Initialize a PartBoot root directory |
| `partboot scan --root <path>` | Discover ISO images |
| `partboot extract --root <path> --iso <name>` | Extract boot files from an ISO |
| `partboot menu --root <path> --uuid <uuid>` | Generate GRUB configuration |
| `partboot stage --root <path> --grub-x64 <path>` | Stage EFI binaries |
| `partboot esp --root <path> --esp <path> --force` | Install EFI files to the ESP |
| `partboot doctor --root <path> [--esp <path>]` | Run health checks |
| `partboot boot --esp <path>` | Show manual boot instructions |

### Boot Entry Management

| Command | Description |
|---|---|
| `partboot entry list` | List firmware boot entries |
| `partboot entry create --esp <path> --label <name> --root <path>` | Create a UEFI boot entry |
| `partboot entry remove --id <guid>` | Remove a firmware boot entry |
| `partboot entry restore --backup <path>` | Restore a BCD backup |

### Common Flags

| Flag | Description |
|---|---|
| `--json` | Machine-readable output |
| `--dry-run` | Validate inputs without making changes |
| `--skip-entry` | Skip firmware boot entry creation |
| `--diagnostics` | Add a diagnostics entry to the GRUB menu |
| `-h, --help` | Display help |
| `-V, --version` | Display version |

> [!TIP]
> Get per-command help with `partboot <command> --help` or `partboot entry <subcommand> --help`.

## Scripted Workflow

For non-interactive use, provide all parameters to `start`:

```powershell
partboot start `
  --root H:\partboot `
  --esp S:\ `
  --partition-uuid 9412B8E612B8CF0C `
  --partition-label PARTBOOT `
  --include-diagnostics `
  --json
```

## Directory Layout

PartBoot creates this structure under the selected root:

```
H:\partboot
  isos\        ISO images
  cache\       cached EFI binaries
  extracted\   extracted boot files
  profiles\    per-ISO boot profiles
  efi\         staged EFI files
  generated\   generated GRUB configuration
```

## Building from Source

Source builds are only needed when you want to modify PartBoot or test an unreleased change.

### Prerequisites

- Rust 1.95 or newer
- Git
- Microsoft C++ Build Tools or Visual Studio with the C++ workload (MSVC toolchain)
- 7-Zip for ISO extraction and tests

### Build

```powershell
cargo build --release
```

The binary is at `target\release\partboot.exe`.

### Test and Lint

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

If using the `x86_64-pc-windows-gnu` toolchain, ensure `dlltool.exe` is on `PATH` (e.g., `C:\msys64\ucrt64\bin`).

## Known Limitations

- Windows ISOs are detected but not yet bootable
- Secure Boot must be disabled unless you provide your own signing flow
- Extracted-first boot profiles are planned but not active for all distros

See [Future Work](docs/future-work.md) for the full list of planned improvements.

## Documentation

- [Usage Guide](docs/usage.md) — supported ISO families, partition guidance, troubleshooting
- [Developer Guide](DEVELOPMENT.md) — build, test, release, and implementation notes
- [GRUB Strategy](docs/architecture/grub-strategy.md) — EFI and GRUB architecture notes
