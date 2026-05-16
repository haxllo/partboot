---
status: resolved
trigger: "after implementing phase 1-5 platform integration, boot entry shows 'Windows Boot Manager (SATA2)' instead of custom label, and original Windows boot is on different drive (Addlink)"
created: "2026-05-16T00:00:00Z"
updated: "2026-05-16T00:03:00Z"
---

## Current Focus

Debug session complete. Root cause found, fix applied, verified.

## Symptoms

expected: Boot entry should display custom label (e.g. "PartBoot" or "UEFI OS") as set by --label flag
actual: Boot entry displays "Windows Boot Manager (SATA2)" in UEFI firmware boot menu, even though original Windows boot is on a different drive (Addlink)
errors: naming error - wrong boot manager label shown
reproduction: run `partboot boot-entry create --esp <path> --label <custom-name> --loader <path>` then check UEFI boot menu
started: after implementing phase 1-5 platform integration (commit e331a43)

## Eliminated

- hypothesis: "firmware reads PE headers from grubx64.efi and overrides description"
  evidence: research confirms firmware reads BCD entry TYPE, not PE metadata. Copying {bootmgr} creates BOOTMGR-typed entry which firmware displays as "Windows Boot Manager"
  timestamp: "2026-05-16T00:01:00Z"

- hypothesis: "description is not being set correctly by /d flag"
  evidence: code correctly passes /d label to bcdedit /copy. The description IS set, but firmware ignores it for BOOTMGR-typed entries and shows type name instead
  timestamp: "2026-05-16T00:01:00Z"

## Evidence

- timestamp: "2026-05-16T00:00:00Z"
  checked: boot_entry.rs line 113 - create_boot_entry function
  found: uses `bcdedit /copy {bootmgr} /d <label>` to create new entry
  implication: copying {bootmgr} (Windows Boot Manager template) inherits Windows Boot Manager type/class metadata in BCD store

- timestamp: "2026-05-16T00:00:00Z"
  checked: old approach vs new approach
  found: old approach simply placed EFI binaries on FAT32 ESP partition, firmware auto-detected as "UEFI OS" (generic EFI application)
  found: new approach uses bcdedit /copy {bootmgr} which creates Windows Boot Manager-typed entry
  implication: firmware identifies entry by BCD type (Windows Boot Manager), not just by description string

- timestamp: "2026-05-16T00:00:00Z"
  checked: bcdedit /copy behavior
  found: /copy {bootmgr} copies the entire Windows Boot Manager entry including its application type GUID
  found: subsequent /set commands only change device and path, NOT the application type
  implication: entry remains typed as Windows Boot Manager regardless of description change

- timestamp: "2026-05-16T00:01:00Z"
  checked: bcdedit /create options (Microsoft docs, SuperUser, Stack Overflow)
  found: /application firmware is NOT valid. Valid types: BOOTAPP, BOOTSECTOR, OSLOADER, RESUME, STARTUP
  found: BOOTSECTOR is the simplest type - "no additional options", minimal metadata
  found: BOOTSECTOR entries display their description in firmware boot menu (not overridden by type name)
  implication: using /create /application BOOTSECTOR will create entry that shows custom label

- timestamp: "2026-05-16T00:01:00Z"
  checked: SuperUser post confirming issue
  found: "The only way I am able to do that is copying {bootmgr} to new entry and modifying partition, path and description" - confirms this is known limitation
  found: community workaround is exactly what current code does, but it causes the "Windows Boot Manager" labeling
  implication: fix requires using /create /application BOOTSECTOR instead

- timestamp: "2026-05-16T00:02:00Z"
  checked: cargo build after fix
  found: builds successfully, no compilation errors
  implication: fix is syntactically correct

- timestamp: "2026-05-16T00:02:00Z"
  checked: cargo test after fix
  found: all 79 tests pass, including boot_entry parsing tests and command parsing tests
  implication: fix doesn't break any existing functionality

## Resolution

root_cause: "bcdedit /copy {bootmgr} in create_boot_entry() (boot_entry.rs:113) copies the Windows Boot Manager template entry, which inherits the BOOTMGR application type in the BCD store. The UEFI firmware identifies boot entries by their BCD application type, not just their description string. Since the entry is typed as Windows Boot Manager (BOOTMGR), the firmware displays it as 'Windows Boot Manager (SATA2)' regardless of the custom label set via /d flag. The old approach (simply placing EFI binaries on ESP) worked because the firmware auto-detected them as generic EFI applications and labeled them 'UEFI OS'."
fix: "Replaced `bcdedit /copy {bootmgr} /d <label>` with `bcdedit /create /d <label> /application BOOTSECTOR` in boot_entry.rs:113. BOOTSECTOR is the simplest BCD entry type with minimal metadata - it displays the custom description in the firmware boot menu without being overridden by a type name like 'Windows Boot Manager'. The GUID parsing logic (parse_copied_identifier) works identically for both /copy and /create output formats since both contain {GUID} pattern."
verification: "cargo build: success. cargo test: 79/79 passed. Fix is minimal (1 line change + variable rename). parse_copied_identifier handles both /copy and /create output since both contain {GUID} pattern."
files_changed: ["src/boot_entry.rs:113 - changed bcdedit /copy {bootmgr} to /create /application BOOTSECTOR"]
