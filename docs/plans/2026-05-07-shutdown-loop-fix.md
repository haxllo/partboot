# Shutdown Loop Fix Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make Ubuntu live-ISO boot default to the tested RAM-backed mode that avoids shutdown hangs caused by loop-backed ISO teardown.

**Architecture:** Keep the current working GRUB boot path, but make Ubuntu-style Casper ISOs boot with `toram noprompt` by default. Testing showed normal and `noprompt` modes still leave the live system dependent on the ISO-backed loop device during shutdown, while `toram` shuts down cleanly.

**Tech Stack:** Rust 2021, standard library only, GRUB2, Ubuntu Casper boot parameters.

---

### Task 1: Capture The Root Cause In Docs

**Files:**
- Modify: `README.md`

**Step 1: Add troubleshooting section**

Document that successful boot plus shutdown `I/O error, dev loop0` means firmware and GRUB worked, but Ubuntu live teardown failed while the ISO-backed loop device was still active.

**Step 2: Add immediate workaround**

Document trying the new safe shutdown or RAM-backed menu entry before forcing power off.

**Step 3: Verify**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: PASS.

### Task 2: Make Ubuntu RAM Mode The Default

**Files:**
- Modify: `src/grub.rs`

**Step 1: Write failing test**

Update the Ubuntu test so the default entry contains:

```text
menuentry 'ubuntu-22.04.5-desktop-amd64.iso'
toram noprompt
```

**Step 2: Run test**

Run: `cargo +stable-x86_64-pc-windows-gnu test ubuntu_grub_entry_contains_loopback_boot`
Expected: FAIL.

**Step 3: Implement default**

Generate one Ubuntu entry with:

```text
boot=casper iso-scan/filename=$isofile toram noprompt quiet splash ---
```

**Step 4: Run tests**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: PASS.

### Task 3: Remove Failed Experimental Entries

**Files:**
- Modify: `src/grub.rs`

**Step 1: Write failing test**

Assert the generated Ubuntu config does not contain:

```text
(safe shutdown)
(copy to RAM)
(debug)
```

**Step 2: Run test**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: FAIL.

**Step 3: Remove variants**

Remove the normal, safe-shutdown, and debug variants from the default generated menu.

**Step 4: Run tests**

Run: `cargo +stable-x86_64-pc-windows-gnu test`
Expected: PASS.

### Task 4: Regenerate And Reinstall Test Config

**Files:**
- Generated: `H:\partboot\generated\grub.cfg`
- Generated: `H:\partboot\efi\EFI\PartBoot\grub.cfg`
- Generated: `S:\EFI\PartBoot\grub.cfg`
- Generated: `S:\EFI\Boot\grub.cfg`

**Step 1: Generate menu**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- generate-menu --root H:\partboot --partition-uuid 12B8CF0C --partition-label partboottest
```

**Step 2: Stage EFI**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- stage-efi --root H:\partboot --grub-x64 H:\partboot\cache\grubx64.efi --boot-x64 H:\partboot\cache\bootx64.efi
```

**Step 3: Install ESP paths**

Run:

```powershell
cargo +stable-x86_64-pc-windows-gnu run -- install-esp --root H:\partboot --esp S:\ --force
cargo +stable-x86_64-pc-windows-gnu run -- install-fallback --root H:\partboot --esp S:\ --force
```

### Task 5: Boot Test

**Files:**
- Modify: `README.md`

**Step 1: Test default entry**

Boot Ubuntu default entry. Shut down cleanly. Expected: no endless loop0 I/O spam.

**Step 2: Record RAM requirement**

Document that this mode needs enough RAM for the ISO and live runtime.

### Task 6: Next Leap

**Files:**
- Create: `src/extract.rs`
- Modify: `src/main.rs`
- Modify: `src/grub.rs`

**Step 1: Add extracted-casper command**

Add:

```powershell
partboot extract --root H:\partboot --iso ubuntu-22.04.5-desktop-amd64.iso
```

It should extract `casper/vmlinuz`, `casper/initrd`, and `casper/filesystem.squashfs` into:

```text
H:\partboot\extracted\ubuntu-22.04.5-desktop-amd64\
```

**Step 2: Generate extracted boot entry**

Boot kernel/initrd directly from extracted files and point Casper at the extracted squashfs/root medium.

**Step 3: Re-test shutdown**

Expected: comparable shutdown stability without requiring enough RAM to copy the whole live image.
