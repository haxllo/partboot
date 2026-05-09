# PartBoot

PartBoot is a disk-resident ISO boot manager design and prototype. The goal is
to boot ISO images from a selected internal SSD/HDD partition using a small boot
entry and generated GRUB configuration, instead of requiring a dedicated USB
flash drive.

The current MVP is intentionally conservative: it does not repartition disks,
write firmware boot entries, install GRUB, or modify an EFI System Partition. It
creates a PartBoot directory layout, scans ISO files, classifies common images,
and generates a GRUB config that can be inspected before any bootloader work is
attempted.

## Current Commands

Run commands in this repo with the stable GNU toolchain prefix:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- <subcommand>
```

```powershell
cargo run -- init --root <ROOT_PATH>
cargo run -- scan --root <ROOT_PATH>
cargo run -- extract --root <ROOT_PATH> --iso <ISO_NAME_OR_PATH>
cargo run -- volume-id --drive <DRIVE_LETTER:>
cargo run -- generate-menu --root <ROOT_PATH> --partition-uuid <PARTITION_UUID> --partition-label <PARTITION_LABEL>
cargo run -- stage-efi --root <ROOT_PATH> --grub-x64 <GRUB_X64_PATH> --boot-x64 <BOOT_X64_PATH>
cargo run -- install-esp --root <ROOT_PATH> --esp <ESP_PATH> --force
cargo run -- install-fallback --root <ROOT_PATH> --esp <ESP_PATH> --force
cargo run -- boot-instructions --esp <ESP_PATH>
cargo run -- doctor --root <ROOT_PATH> --esp <ESP_PATH>
cargo run -- guided-test-flow --root <ROOT_PATH> --esp <ESP_PATH> --partition-uuid <PARTITION_UUID> --partition-label <PARTITION_LABEL>
cargo run -- start
```

Use the real value printed by `volume-id` in place of `<PARTITION_UUID>`. For NTFS,
run `volume-id` or `fsutil fsinfo ntfsinfo <DRIVE_LETTER:>` from an elevated terminal so the
full NTFS serial is available. If the ISO partition has a stable label, pass it
with `--partition-label` so GRUB has a fallback when firmware/GRUB reports an
NTFS UUID differently from Windows.
Short NTFS serial values (for example `12B8CF0C`) are not accepted for menu
generation; use the full NTFS UUID (for example `9412B8E612B8CF0C`).
If admin rights are required for `fsutil`, PartBoot now auto-prompts for UAC and
re-runs the command elevated.

Expected directory layout:

```text
H:\partboot
├─ isos\
├─ profiles\
├─ cache\
├─ extracted\
│  └─ ubuntu-22.04.5-desktop-amd64\
│     └─ casper\
│        ├─ filesystem.squashfs
│        ├─ initrd
│        └─ vmlinuz
├─ efi\
│  └─ EFI\
│     └─ PartBoot\
│        ├─ bootx64.efi
│        ├─ grubx64.efi
│        ├─ grub.cfg
│        └─ README.txt
└─ generated\
   └─ grub.cfg
```

`stage-efi` copies the generated GRUB config, a supplied `grubx64.efi`, and
optionally a `bootx64.efi` shim/fallback loader into
`<root>\efi\EFI\PartBoot`. It does not copy anything to the real EFI System
Partition and does not create firmware boot entries.

`install-esp` copies staged files into an explicitly supplied FAT32 ESP/test
partition path. It requires either `--dry-run` or `--force`, validates FAT32 on
Windows, and only writes under `EFI\PartBoot`. It still does not create firmware
boot entries.

`boot-instructions` validates the copied EFI files and prints the manual
firmware boot path. Use this before adding any persistent boot entry.

`install-fallback` copies the staged loader to the standard UEFI fallback path
`EFI\Boot\bootx64.efi`. Use this when firmware does not provide a file browser;
after installing it, reboot and choose the UEFI entry for that disk/partition.

`extract` uses `7z` to extract Ubuntu Casper files from an ISO. When a complete
extracted Casper directory exists, `generate-menu` emits an extracted boot entry
and an ISO RAM fallback entry. PartBoot also creates per-ISO profile files in
`partboot/profiles` for Ubuntu images and uses them to decide preferred/fallback
menu behavior.
If `7z` is not in PATH, set `PARTBOOT_7Z_PATH` to the 7z executable location.

The generated GRUB menu keeps entry labels minimal: each main entry uses only
the ISO name, and fallback entries use `[Fallback]`. Diagnostics are hidden by
default and are only included when `--include-diagnostics` is passed.

## UX workflows

Quick path (single command):

```powershell
cargo run -- guided-test-flow --root <ROOT_PATH> --esp <ESP_PATH> --partition-uuid <PARTITION_UUID> --partition-label <PARTITION_LABEL>
```

Interactive quick path (detect partitions, prompt for selections):

```powershell
cargo run -- start
```

`start` auto-imports ISO files from the selected drive root (for example
`H:\ubuntu.iso`) into `H:\partboot\isos\` when `partboot\isos` is empty. It
tries move-first (same drive) to avoid requiring double disk space.
`start` now also auto-populates `H:\partboot\cache` from bundled EFI assets
(`assets\efi`) when cache binaries are missing, after checksum verification.
Override asset location with `PARTBOOT_EFI_ASSETS`.

## Release packaging

Build a release bundle with bundled EFI assets:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu
```

If EFI binaries were replaced, regenerate and verify checksums while packaging:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -RefreshChecksums
```

See `docs/release-efi-provenance.md` for required provenance notes per release.

Safe step-by-step path:

```powershell
cargo run -- init --root <ROOT_PATH>
cargo run -- scan --root <ROOT_PATH>
cargo run -- extract --root <ROOT_PATH> --iso <ISO_NAME_OR_PATH>
cargo run -- generate-menu --root <ROOT_PATH> --partition-uuid <PARTITION_UUID> --partition-label <PARTITION_LABEL>
cargo run -- stage-efi --root <ROOT_PATH> --grub-x64 <ROOT_PATH>\cache\grubx64.efi --boot-x64 <ROOT_PATH>\cache\bootx64.efi
cargo run -- install-esp --root <ROOT_PATH> --esp <ESP_PATH> --force
cargo run -- install-fallback --root <ROOT_PATH> --esp <ESP_PATH> --force
cargo run -- doctor --root <ROOT_PATH> --esp <ESP_PATH>
```

For automation, `scan`, `generate-menu`, `doctor`, and `guided-test-flow` support `--json`.

## ISO Support

Supported in the generated GRUB menu:

- Ubuntu-style Casper live ISOs
- Debian/Kali-style live ISOs
- Arch-style live ISOs
- Fedora-style live ISOs

Experimental or blocked:

- Windows installer ISOs are detected, but the MVP emits a disabled menu entry.
  Windows support needs a `wimboot` or extracted-installer backend.
- Unknown ISOs are detected, but need explicit boot profiles.

## Testing Partition Recommendation

Yes, create a separate disposable partition for testing.

Start with **one NTFS partition** around 16-64 GB. That is the safest first
target from Windows because it supports large ISO files and is easy to inspect.
Use it only for PartBoot testing, for example as `H:\partboot`.

Add filesystems in this order:

1. **NTFS** first: best first test target on Windows; supports large ISOs.
2. **FAT32** later: useful for EFI-file experiments, but cannot store files over
   4 GB.
3. **ext4** later: useful for Linux-first testing, but Windows will not manage it
   comfortably.

Do not test on a partition that contains personal data, an installed OS, or a
recovery image.

## Development Notes

The implementation plan is in
`docs/plans/2026-05-08-partboot-mvp.md`.

The shutdown loop follow-up plan is in
`docs/plans/2026-05-07-shutdown-loop-fix.md`.

## Troubleshooting

### Common command errors

- `error: missing or unreadable checksum manifest ... assets\efi\checksums.txt`  
  Cause: bundled EFI checksum manifest is missing.  
  Fix:

  ```powershell
  powershell -ExecutionPolicy Bypass -File .\scripts\package-release.ps1 -Target x86_64-pc-windows-gnu -RefreshChecksums
  ```

- `rustc ... is not supported` while running cargo commands  
  Cause: wrong/default toolchain.  
  Fix: run with `+stable-x86_64-pc-windows-gnu` prefix.

### Ubuntu Boots But Shutdown Shows loop0 I/O Errors

If Ubuntu boots from the PartBoot menu but shutdown or restart repeatedly prints
messages like:

```text
I/O error, dev loop0, sector 0
```

then firmware boot, GRUB loading, partition discovery, and kernel startup all
worked. The failure is later: Ubuntu's live session is tearing down a root
filesystem that still depends on an ISO-backed loop device.

The current Ubuntu entry uses:

```text
boot=casper iso-scan/filename=...
```

That boots successfully, but the live system still depends on the ISO file on
the NTFS partition during shutdown. Testing showed that the `toram` path shuts
down cleanly, so PartBoot now makes Ubuntu-style Casper entries use:

```text
boot=casper iso-scan/filename=... toram noprompt
```

This keeps ISO storage on NTFS while reducing shutdown dependency on the ISO
loop device. It requires enough RAM to hold the live image and runtime.

The preferred next mode is extracted Casper. After running `extract`, PartBoot
boots `vmlinuz`, `initrd`, and `filesystem.squashfs` from
`partboot/extracted/<iso-id>/casper`. This should reduce RAM pressure while
still avoiding whole-ISO loop teardown during shutdown. Extracted mode is still
experimental; it passes `ignore_uuid` because the extracted directory does not
carry the ISO's full `.disk` metadata. If it fails, choose the generated ISO RAM
fallback entry.
