# PartBoot

PartBoot is a disk-resident ISO boot manager for UEFI systems. It lets you keep Linux ISO images on a local partition and boot them through a generated GRUB menu instead of preparing a USB drive for each installer or live image.

> PartBoot is early-stage software. Test it on a disposable partition first and do not point it at a partition that contains personal data, an installed operating system, or recovery media.

## Features

- Interactive `partboot start` workflow for scanning, extracting, menu generation, and EFI staging.
- ISO discovery and boot profiles for Ubuntu, Debian/Kali, Arch, Fedora, and other GRUB-compatible Linux live images.
- Generated GRUB configuration with optional diagnostics.
- EFI staging and install helpers with explicit `--dry-run` / `--force` safeguards.
- JSON output for automation-oriented commands.

Windows installer ISOs are detected but not booted yet. See [Future Work](docs/future-work.md).

## Runtime Requirements

- Windows with UEFI firmware.
- 7-Zip installed or `PARTBOOT_7Z_PATH` pointing to `7z.exe`.
- A separate NTFS test partition, recommended size 16-64 GB.
- Secure Boot disabled unless you provide your own trusted EFI signing flow.

## Installation

Install with WinGet:

```powershell
winget install --id Haxllo.PartBoot --exact
```

WinGet can lag the latest GitHub or crates.io release while the package submission is reviewed.

Install with Cargo:

```powershell
cargo install partboot
```

`cargo install` builds from source, so it requires the Rust and C++ build tools listed below.

Or download `partboot.exe` from the [GitHub Releases](https://github.com/haxllo/partboot/releases) page.

## Building from Source

Source builds are only needed when you want to modify PartBoot or test an unreleased change.

Build requirements:

- Rust 1.95 or newer.
- Git.
- Microsoft C++ Build Tools or Visual Studio with the C++ workload when using the default Windows MSVC Rust toolchain.
- 7-Zip for ISO extraction tests and local end-to-end runs.

Build the binary:

```powershell
cargo build --release
```

Run it from `target\release\partboot.exe`.

## Quick Start

Start the guided workflow:

```powershell
partboot start
```

The wizard will:

1. Detect available partitions.
2. Import ISO files into the selected PartBoot directory.
3. Extract boot files from supported Linux ISOs.
4. Generate a GRUB boot menu.
5. Stage EFI files and print the installation instructions.

After reviewing the generated files, install them to an EFI system partition:

```powershell
partboot install-esp --root <PARTBOOT_ROOT> --esp <ESP_PATH> --force
partboot boot-instructions --esp <ESP_PATH>
```

Then reboot and select the PartBoot entry from the firmware boot menu.

## Documentation

- [Usage Guide](docs/usage.md): command reference, supported ISO families, partition guidance, and troubleshooting.
- [Developer Guide](DEVELOPMENT.md): build, test, release, and implementation notes.
- [GRUB Strategy](docs/architecture/grub-strategy.md): EFI and GRUB architecture notes.
- [Planning Status](docs/plans/README.md): implementation status for active and historical plan docs.
- [Future Work](docs/future-work.md): known limitations and planned improvements.
- [Contributing](CONTRIBUTING.md): contribution workflow and quality expectations.

## License

PartBoot is licensed under the [MIT License](LICENSE).
