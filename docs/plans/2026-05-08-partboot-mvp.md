# PartBoot MVP Implementation Plan

## Status Audit (2026-05-15)

Overall status: `Done`

Task status:
- [x] Task 1: Project scaffold and dependency-free CLI parser implemented.
- [x] Task 2: Layout initializer implemented (`isos`, `profiles`, `cache`, `generated`, plus `extracted`).
- [x] Task 3: ISO scanner and family classification implemented.
- [x] Task 4: GRUB config generation implemented for core Linux families with unsupported-entry handling.
- [x] Task 5: Safety and non-goal documentation present in user docs.

Verification snapshot:
- `cargo +stable-x86_64-pc-windows-gnu test` passes (36 tests).

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a safe first CLI slice that prepares a PartBoot ISO directory, scans ISO images, and generates a GRUB menu for booting supported Linux ISOs from a selected disk partition.

**Architecture:** The MVP does not install bootloaders or modify disks. It models the disk-resident boot flow by generating deterministic filesystem layout and GRUB configuration from ISO metadata. Destructive operations are deferred until the scanner, profile format, and generated boot menu are tested.

**Tech Stack:** Rust 2021, standard library only, GRUB2 loopback boot entries, JSON-like profile files generated manually for now.

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `README.md`

**Step 1: Write the failing test**

Create a CLI unit test that parses `init --root X:\partboot` into an `Init` command.

**Step 2: Run test to verify it fails**

Run: `cargo test parse_init_command`
Expected: FAIL because the parser does not exist.

**Step 3: Write minimal implementation**

Implement a small dependency-free argument parser for `init`, `scan`, `generate-menu`, `doctor`, and `recommend-test-partitions`.

**Step 4: Run test to verify it passes**

Run: `cargo test parse_init_command`
Expected: PASS.

### Task 2: Repository Layout Initializer

**Files:**
- Modify: `src/main.rs`
- Create: `src/layout.rs`

**Step 1: Write the failing test**

Test that layout paths resolve to `partboot/isos`, `partboot/profiles`, `partboot/cache`, and `partboot/generated`.

**Step 2: Run test**

Run: `cargo test layout_paths_are_stable`
Expected: FAIL.

**Step 3: Implement**

Add `PartBootLayout` with `ensure()` that creates the directories.

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS.

### Task 3: ISO Scanner

**Files:**
- Create: `src/iso.rs`
- Modify: `src/main.rs`

**Step 1: Write the failing test**

Test classification for `ubuntu-24.04.iso`, `archlinux.iso`, `fedora.iso`, and `Win11.iso`.

**Step 2: Run test**

Run: `cargo test classify_known_iso_names`
Expected: FAIL.

**Step 3: Implement**

Scan `isos/` for `.iso`, infer support level and boot family from filename.

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS.

### Task 4: GRUB Config Generator

**Files:**
- Create: `src/grub.rs`
- Modify: `src/main.rs`

**Step 1: Write the failing test**

Test that Ubuntu emits `loopback loop`, `linux (loop)/casper/vmlinuz`, and `iso-scan/filename=`.

**Step 2: Run test**

Run: `cargo test ubuntu_grub_entry_contains_loopback_boot`
Expected: FAIL.

**Step 3: Implement**

Generate GRUB menu entries for known Linux families, and generate disabled explanatory entries for Windows/unknown ISOs.

**Step 4: Run tests**

Run: `cargo test`
Expected: PASS.

### Task 5: Safety Documentation

**Files:**
- Modify: `README.md`

**Step 1: Document test partition guidance**

Recommend one disposable NTFS partition for Windows-side testing, then FAT32 and ext4 later.

**Step 2: Document non-goals**

State that the MVP does not repartition disks, install EFI entries, or promise Windows ISO boot support.

**Step 3: Verify**

Run: `cargo test`
Expected: PASS.
