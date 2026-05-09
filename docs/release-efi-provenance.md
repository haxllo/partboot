# Bundled EFI Binary Provenance

This file records provenance for bundled EFI binaries included in PartBoot release artifacts.

## Files

- `assets/efi/grubx64.efi`
- `assets/efi/bootx64.efi`
- `assets/efi/checksums.txt`

## Required metadata per release

1. Source distribution (example: Ubuntu 24.04.2 ISO).
2. Exact source image file name and hash.
3. Extraction path used (for each EFI file).
4. Local file hash after extraction.
5. Checksum manifest values committed in `checksums.txt`.
6. Signing/trust note (whether file is vendor-signed, shim usage expectations).

## Release update checklist

1. Replace `grubx64.efi` and `bootx64.efi` in `assets/efi`.
2. Run `scripts/package-release.ps1 -RefreshChecksums`.
3. Verify the script reports checksum validation success.
4. Update this file with release-specific provenance notes.
5. Include this file in the packaged release artifact.

## Current release record (v0.1.0)

Status: **provisional** (original source ISO metadata not yet recorded).

- Source distribution: **Unknown (to be confirmed)**
- Source image name/path: **Unknown (to be confirmed)**
- Source image hash (SHA256): **Unknown (to be confirmed)**
- Extraction paths used:
  - `EFI/BOOT/grubx64.efi` -> `assets/efi/grubx64.efi`
  - `EFI/BOOT/BOOTx64.EFI` (or shim fallback equivalent) -> `assets/efi/bootx64.efi`
- Local bundled file hashes (SHA256):
  - `assets/efi/grubx64.efi` = `9329EBF0F4DA03234A6E7349A0C6469B8CBC64EC748E62115643F34C813CE7FE`
  - `assets/efi/bootx64.efi` = `4C89145E958CF592A6F16552EADF112EF2C1C525E2435C2761E6A99FA88188B3`
  - `assets/efi/checksums.txt` = `A056FC36C804A77B33886BA7283F53B30318CC1CABFCA346D5ED402602F229C8`
- Manifest values (CRC32):
  - `grubx64.efi=F24B161C`
  - `bootx64.efi=FAA8033D`
- Signing/trust note:
  - Files are treated as vendor-provided EFI binaries extracted from a distro ISO path.
  - Secure Boot trust is firmware-dependent; keep shim/bootx64 path available and validate on target hardware.

## How to fill missing source metadata

If you still have the source ISO, run:

```powershell
Get-FileHash -Algorithm SHA256 C:\path\to\source.iso
7z l C:\path\to\source.iso | Select-String -Pattern "grubx64.efi|bootx64.efi|shimx64.efi|BOOTx64.EFI"
```

Then replace the `Unknown` fields above with exact values.
