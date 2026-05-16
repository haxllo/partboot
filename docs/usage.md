# Usage Guide

This guide covers day-to-day PartBoot usage. For build and release workflows, see the [Developer Guide](../DEVELOPMENT.md).

## Safe Test Partition

Use a separate disposable partition for PartBoot testing.

Recommended first target:

- Filesystem: NTFS.
- Size: 16-64 GB.
- Example root: `H:\partboot`.

Do not use a partition that contains personal files, an installed operating system, recovery media, or the target location for a Linux installation.

Filesystem notes:

| Filesystem | Use |
| --- | --- |
| NTFS | Best first Windows test target; supports large ISO files. |
| FAT32 | Useful for EFI experiments; cannot store files larger than 4 GB. |
| ext4 | Useful for Linux-first testing; awkward to manage from Windows. |

## Directory Layout

PartBoot creates this layout under the selected root:

```text
H:\partboot
  isos\        ISO images
  cache\       downloaded or cached EFI binaries
  extracted\   extracted boot files
  profiles\    per-ISO boot profiles
  efi\         staged EFI files
  generated\   generated GRUB menu
```

## Quick Workflow

Run the interactive wizard:

```powershell
partboot start
```

Review generated files, then install staged EFI files:

```powershell
partboot install-esp --root H:\partboot --esp S:\ --force
partboot boot-instructions --esp S:\
```

Use `--dry-run` instead of `--force` when validating the target paths:

```powershell
partboot install-esp --root H:\partboot --esp S:\ --dry-run
```

## Command Reference

### `start`

Run the guided TUI workflow.

```powershell
partboot start [--include-diagnostics] [--dry-run-install]
```

### `init`

Create the PartBoot directory layout.

```powershell
partboot init --root H:\partboot
```

### `scan`

Scan `isos\` and create missing profiles.

```powershell
partboot scan --root H:\partboot
partboot scan --root H:\partboot --json
```

### `extract`

Extract boot files from an ISO.

```powershell
partboot extract --root H:\partboot --iso ubuntu-24.04-desktop-amd64.iso
```

The ISO can be a name inside `isos\` or a full path.

### `volume-id`

Print the partition identifier to use in generated GRUB menus.

```powershell
partboot volume-id --drive H:
```

For NTFS, use the full NTFS serial. Short serials such as `12B8CF0C` are rejected because GRUB expects the full identifier, for example `9412B8E612B8CF0C`.

### `generate-menu`

Generate `generated\grub.cfg`.

```powershell
partboot generate-menu --root H:\partboot --partition-uuid 9412B8E612B8CF0C --partition-label PARTBOOT
partboot generate-menu --root H:\partboot --partition-uuid 9412B8E612B8CF0C --include-diagnostics
partboot generate-menu --root H:\partboot --partition-uuid 9412B8E612B8CF0C --json
```

### `stage-efi`

Stage EFI files without writing to a real EFI system partition.

```powershell
partboot stage-efi --root H:\partboot --grub-x64 C:\tmp\grubx64.efi --boot-x64 C:\tmp\bootx64.efi
```

### `install-esp`

Copy staged EFI files to an EFI system partition. The command requires either `--dry-run` or `--force`.

```powershell
partboot install-esp --root H:\partboot --esp S:\ --dry-run
partboot install-esp --root H:\partboot --esp S:\ --force
```

### `install-fallback`

Copy the loader to the UEFI fallback path `EFI\Boot\bootx64.efi`.

```powershell
partboot install-fallback --root H:\partboot --esp S:\ --dry-run
partboot install-fallback --root H:\partboot --esp S:\ --force
```

### `boot-instructions`

Print the firmware boot path.

```powershell
partboot boot-instructions --esp S:\
```

### `doctor`

Check common setup issues.

```powershell
partboot doctor --root H:\partboot
partboot doctor --root H:\partboot --esp S:\
partboot doctor --root H:\partboot --esp S:\ --json
```

### `guided-test-flow`

Run scan, extraction, menu generation, and EFI staging in one command.

```powershell
partboot guided-test-flow --root H:\partboot --esp S:\ --partition-uuid 9412B8E612B8CF0C --partition-label PARTBOOT
```

Optional flags:

- `--iso <name>`: process one ISO.
- `--include-diagnostics`: include diagnostic menu entries.
- `--dry-run-install`: validate install steps without copying to the ESP.
- `--json`: print machine-readable output.

### `recommend-test-partitions`

Print safe test-partition guidance.

```powershell
partboot recommend-test-partitions
```

### `boot-entry list`

List firmware boot entries. Use `--partboot-only` to show only PartBoot-managed entries.

```powershell
partboot boot-entry list
partboot boot-entry list --partboot-only
partboot boot-entry list --json
```

### `boot-entry create`

Create a new UEFI firmware boot entry. Requires an elevated (Administrator) shell unless `--dry-run` is used.

```powershell
partboot boot-entry create --esp S:\ --root H:\partboot --label "PartBoot" --dry-run
partboot boot-entry create --esp S:\ --root H:\partboot --label "PartBoot" --first
partboot boot-entry create --esp S:\ --loader \EFI\PartBoot\grubx64.efi --label "PartBoot" --dry-run
```

Flags:

- `--esp`: EFI system partition path (required).
- `--root`: PartBoot root directory; auto-resolves the loader to `\EFI\PartBoot\grubx64.efi`.
- `--loader`: Explicit ESP-relative loader path (alternative to `--root`).
- `--label`: Human-readable entry name (required).
- `--first`: Add entry to the top of the boot order (default: last).
- `--dry-run`: Validate inputs without modifying firmware.
- `--json`: Machine-readable output.

Safety behavior:

- Before any modification, the full BCD store is exported to `%TEMP%\partboot-bcd-backup-<timestamp>.bak`.
- If an entry with the same label and loader already exists, it is reused instead of duplicated.
- Secure Boot state is detected and reported in the output.
- The command fails if not run as Administrator (unless `--dry-run`).

### `boot-entry remove`

Remove a firmware boot entry by its GUID identifier. Requires an elevated shell unless `--dry-run` is used.

```powershell
partboot boot-entry remove --id "{12345678-1234-1234-1234-123456789ABC}" --dry-run
partboot boot-entry remove --id "{12345678-1234-1234-1234-123456789ABC}"
```

Safety behavior:

- Protected identifiers (`{bootmgr}`, `{fwbootmgr}`, `{current}`, `{default}`, `{memdiag}`) cannot be removed.
- The BCD store is backed up before removal.
- The identifier must be a valid GUID wrapped in braces.

### `boot-entry restore`

Restore a previously exported BCD backup. Requires an elevated shell unless `--dry-run` is used.

```powershell
partboot boot-entry restore --backup C:\Users\Me\AppData\Local\Temp\partboot-bcd-backup-1715000000.bak --dry-run
partboot boot-entry restore --backup C:\Users\Me\AppData\Local\Temp\partboot-bcd-backup-1715000000.bak
```

## Boot Entry Rollback

Every `boot-entry create` and `boot-entry remove` operation exports a full BCD backup before making changes. The backup path is printed in the command output and included in JSON results.

### Rollback after `boot-entry create`

If the created entry causes boot issues:

```powershell
# 1. List current entries to confirm the problematic entry
partboot boot-entry list --partboot-only

# 2. Restore the pre-create BCD backup
partboot boot-entry restore --backup <backup-path-from-create-output>

# 3. Verify restoration
partboot boot-entry list --partboot-only
```

Or manually with bcdedit:

```powershell
bcdedit /import <backup-path-from-create-output>
```

### Rollback after `boot-entry remove`

If an entry was removed by mistake:

```powershell
# 1. Restore the pre-remove BCD backup
partboot boot-entry restore --backup <backup-path-from-remove-output>

# 2. Verify the entry is back
partboot boot-entry list --partboot-only
```

### Manual rollback with bcdedit

If the `partboot boot-entry restore` command is unavailable:

```powershell
# Must be run from an elevated Command Prompt or PowerShell
bcdedit /import "C:\path\to\partboot-bcd-backup-<timestamp>.bak"
```

### Emergency recovery

If the BCD store is corrupted and no backup is available:

```powershell
# Rebuild the BCD store from an elevated recovery environment
bootrec /rebuildbcd
bootrec /fixboot
```

Always keep the backup files printed by `boot-entry create` and `boot-entry remove` until you have confirmed the system boots correctly with the new configuration.

## Supported ISO Families

Generated GRUB entries are supported for:

- Ubuntu-style Casper live ISOs.
- Debian and Kali live ISOs.
- Arch live ISOs.
- Fedora live ISOs.
- Most GRUB-compatible Linux live distributions with compatible boot paths.

Detected but not supported yet:

- Windows installer ISOs.
- Unknown ISOs without explicit boot profiles.

## Troubleshooting

### `Cannot find 7z`

Install 7-Zip or set `PARTBOOT_7Z_PATH`:

```powershell
$env:PARTBOOT_7Z_PATH = "C:\Program Files\7-Zip\7z.exe"
```

### Partition is not detected

- Confirm the partition is mounted and visible in File Explorer.
- Use a drive-letter path such as `H:\partboot`.
- Prefer NTFS for the first test partition.

### Generated menu has no entries

- Place ISO files in `partboot\isos\`.
- Use live desktop ISO variants when testing Ubuntu-style images.
- Run `partboot scan --root H:\partboot` and confirm the ISO family is detected.

### Ubuntu shows shutdown errors

This can happen with ISO boot modes because the live session still depends on files from the ISO during shutdown. Save your work and shut down normally; avoid force resets unless the system is already stuck.
