# Tailored GRUB Menu Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the generated GRUB menu clearly branded for PartBoot, with preferred/fallback labels and diagnostics.

**Architecture:** Keep using GRUB as the bootloader, but own the generated `grub.cfg` UX. The generator should emit a PartBoot header, label entries by boot strategy, and include a diagnostics menu entry that prints partition and path information.

**Tech Stack:** Rust 2021, standard library only, generated GRUB2 configuration.

---

### Task 1: Header

**Files:**
- Modify: `src/grub.rs`

**Step 1: Write test**

Assert generated config contains:

```text
set menu_color_normal=white/black
PartBoot ISO Manager
```

**Step 2: Implement**

Add a header function that emits menu colors, title, timeout, default, and modules.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`.

### Task 2: Entry Labels

**Files:**
- Modify: `src/grub.rs`

**Step 1: Write test**

Assert extracted Ubuntu entry contains:

```text
PartBoot | Ubuntu | ubuntu-24.04.iso [Preferred: extracted]
PartBoot | Ubuntu | ubuntu-24.04.iso [Fallback: ISO RAM]
```

**Step 2: Implement**

Rename generated Ubuntu menu entries while preserving boot parameters.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`.

### Task 3: Diagnostics Entry

**Files:**
- Modify: `src/grub.rs`

**Step 1: Write test**

Assert generated config contains:

```text
PartBoot diagnostics
partboot_uuid
partboot_root
```

**Step 2: Implement**

Add a final diagnostics entry that prints UUID, label, GRUB root, and expected directories.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`.

### Task 4: Regenerate And Install

**Files:**
- Generated: `H:\partboot\generated\grub.cfg`
- Generated: `S:\EFI\PartBoot\grub.cfg`
- Generated: `S:\EFI\Boot\grub.cfg`

**Step 1: Generate**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- generate-menu --root H:\partboot --partition-uuid 9412B8E612B8CF0C --partition-label partboottest
```

**Step 2: Stage and install**

Run existing `stage-efi`, `install-esp`, and `install-fallback` commands.

**Step 3: Verify**

Inspect installed `grub.cfg` for branded labels and diagnostics.
