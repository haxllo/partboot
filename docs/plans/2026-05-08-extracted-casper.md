# Extracted Casper Implementation Plan

## Status Audit (2026-05-15)

Overall status: `Partial`

Task status:
- [x] Task 1: Layout includes `extracted` path and stable-path test coverage.
- [x] Task 2: Extracted-id metadata and extracted completeness helpers implemented.
- [x] Task 3: `extract` command implemented with 7-Zip candidate/fallback logic.
- [ ] Task 4: GRUB currently does not prefer extracted entries; Ubuntu still boots via ISO `toram` path.
- [~] Task 5: Regenerate/stage/install steps are operational/manual.
- [~] Task 6: Boot behavior validation is operational/manual and distro-dependent.

Code note:
- The test `ubuntu_grub_entry_stays_single_even_when_extracted_exists` currently confirms extracted mode is not used in generated menu.

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add an extracted Ubuntu Casper boot path so PartBoot can avoid whole-ISO `toram` boot when extracted live files are available.

**Architecture:** Keep ISO storage as the source of truth, but add `partboot extract` to copy selected Casper files into `partboot/extracted/<iso-id>/casper`. Menu generation detects complete extracted directories and emits a direct Casper boot entry; otherwise it falls back to the tested ISO `toram` entry.

**Tech Stack:** Rust 2021, standard library only, external `7z` command for ISO extraction, GRUB2, Ubuntu Casper boot parameters.

---

### Task 1: Layout Support

**Files:**
- Modify: `src/layout.rs`

**Step 1: Write test**

Update `layout_paths_are_stable` to assert:

```rust
assert_eq!(display_path(&layout.extracted), "X:/partboot/extracted");
```

**Step 2: Implement**

Add `extracted: PathBuf` to `PartBootLayout` and create it in `ensure()`.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test layout_paths_are_stable`
Expected: PASS.

### Task 2: Extracted Metadata

**Files:**
- Modify: `src/iso.rs`
- Create: `src/extract.rs`

**Step 1: Write tests**

Test that `ubuntu-22.04.5-desktop-amd64.iso` maps to extracted id:

```text
ubuntu-22.04.5-desktop-amd64
```

**Step 2: Implement**

Add `extracted_id: Option<String>` to `IsoImage`. Add helpers to sanitize ISO names and detect complete extracted Casper directories.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: PASS.

### Task 3: Extract Command

**Files:**
- Modify: `src/main.rs`
- Create: `src/extract.rs`

**Step 1: Write parse test**

Add:

```powershell
partboot extract --root H:\partboot --iso ubuntu-22.04.5-desktop-amd64.iso
```

**Step 2: Implement**

Resolve ISO by exact filename under `partboot/isos` unless an absolute path is supplied. Use `7z e` to extract:

```text
casper\vmlinuz
casper\initrd
casper\filesystem.squashfs
```

into:

```text
partboot\extracted\<iso-id>\casper
```

**Step 3: Verify**

Run command against `H:\partboot`.
Expected: extracted files exist.

### Task 4: GRUB Preference

**Files:**
- Modify: `src/main.rs`
- Modify: `src/grub.rs`

**Step 1: Write test**

Generate config for an Ubuntu `IsoImage` with `extracted_id` and assert it contains:

```text
live-media-path=/partboot/extracted/ubuntu-24.04/casper
```

and does not contain:

```text
loopback loop
toram
```

**Step 2: Implement**

Before menu generation, mark scanned ISO images as extracted when complete extracted files exist.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: PASS.

### Task 5: Regenerate And Install

**Files:**
- Generated: `H:\partboot\generated\grub.cfg`
- Generated: `S:\EFI\PartBoot\grub.cfg`
- Generated: `S:\EFI\Boot\grub.cfg`

**Step 1: Extract Ubuntu**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- extract --root H:\partboot --iso ubuntu-22.04.5-desktop-amd64.iso
```

**Step 2: Regenerate menu**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- generate-menu --root H:\partboot --partition-uuid 12B8CF0C --partition-label partboottest
```

Expected: Ubuntu entry uses `live-media-path`.

**Step 3: Stage and install**

Run existing `stage-efi`, `install-esp`, and `install-fallback` commands.

### Task 6: Boot Test

Boot the default Ubuntu entry. Expected behavior:

- Ubuntu boots without copying the whole ISO into RAM.
- Shutdown completes cleanly.

If shutdown fails, remove the extracted directory and regenerate; the menu falls back to ISO `toram`.
