# Direction 6: Release & Packaging Plan

## Status Audit (2026-05-15)

Overall status: `Partial (implementation largely complete, final validation pending)`

Task status:
- [x] Task 1: Packaged layout defined under `assets/efi` and release bundle script.
- [x] Task 2: Runtime resolver implemented (cache -> bundled assets -> release fallback download).
- [x] Task 3: Checksum verification implemented (`checksums.txt` + CRC32 checks before copy/use).
- [x] Task 4: Guided flow/start path wired through resolver with explicit error messaging.
- [x] Task 5: Packaging automation and provenance/checksum checks implemented.

Remaining:
- [~] Clean-machine validation run and recorded release provenance for the final artifact set.

## Goal

Make first-run setup work without manual cache preparation by bundling required EFI binaries in release artifacts, while keeping provenance and verification explicit.

## Scope

### In scope
- Bundle `grubx64.efi` and `bootx64.efi` in release packages.
- On first run, copy bundled binaries into `<root>\cache` if cache files are missing.
- Add checksum metadata for bundled binaries and verify before use.
- Document bundled binary source/version/signing status in release notes and README.

### Out of scope
- GRUB source fork/custom GRUB build.
- Persistent UEFI boot entry automation.
- Secure Boot signing pipeline redesign.

## Implementation tasks

1. **Define packaged layout**
   - Add a stable release directory layout for bundled EFI binaries (e.g., `assets\efi\`).
   - Ensure runtime path resolution works from installed location.

2. **Add binary resolver**
   - Implement runtime resolver that checks:
     1. `<root>\cache\grubx64.efi` + `<root>\cache\bootx64.efi`
     2. bundled asset location
   - If cache missing and bundled assets present, copy bundled assets into cache.

3. **Add verification**
   - Add embedded checksums (or manifest file) for bundled binaries.
   - Verify checksum before copying/using binaries.
   - Fail with explicit error if verification fails.

4. **Wire into guided flow**
   - Update `start`/guided flow to call resolver before `stage-efi`.
   - Keep clear warning messages when fallback/manual action is required.

5. **Release process updates**
   - Update packaging scripts to include EFI binaries and checksum metadata.
   - Document artifact contents and provenance.

## Progress

- Implemented runtime bundled EFI resolver in guided flow:
  - checks cache first,
  - falls back to bundled assets (`assets\efi` or `PARTBOOT_EFI_ASSETS`),
  - verifies checksum manifest before copy.
- Added `assets\efi\README.txt` with expected bundled layout and checksum format.
- Added release packaging script: `scripts\package-release.ps1`.
- Added release provenance template: `docs\release-efi-provenance.md`.
- Validated packaging script by producing `dist\partboot-0.1.0-x86_64-pc-windows-gnu.zip`.
- Remaining validation: run clean-machine release bundle test and record final provenance values.

## Hardcoded/runtime cleanup notes (already addressed)
- Runtime version string now uses Cargo package version (no hardcoded `0.1.0` string).
- Boot instructions no longer hardcode a specific Ubuntu ISO filename.
- 7-Zip absolute install path removed; runtime uses PATH + optional `PARTBOOT_7Z_PATH`.

## Completion criteria
- Fresh machine with release artifact can run `start` without manual cache file copy.
- Bundled EFI binaries are checksum-verified before use.
- Docs/release notes clearly describe bundled binary origin and trust assumptions.
