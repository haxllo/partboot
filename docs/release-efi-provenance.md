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
2. Rebuild `grubx64.efi` as standalone GRUB (`scripts/build-standalone-grub.ps1`).
3. Run `scripts/package-release.ps1 -RefreshChecksums`.
4. Ensure provenance fields below are fully filled (no `Unknown` values; not `provisional`).
5. Verify the script reports checksum validation success.
6. Update this file with release-specific provenance notes.
7. Include this file in the packaged release artifact.

## Packaging gate behavior

`scripts/package-release.ps1` now fails by default when:

- provenance status is `provisional`
- any required provenance field is `Unknown (to be confirmed)`
- `bootx64.efi` is unsigned or has missing signer metadata

Local-only bypass flags exist for non-release testing:

- `-SkipStandaloneGrubBuild`
- `-SkipProvenanceCheck`

## Current release record (v0.2.2)

Status: **complete**

- Source distribution: **Ubuntu 25.10 desktop amd64**
- Source image name/path: `E:\ubuntu-25.10-desktop-amd64.iso`
- Source image hash (SHA256): `32E30D72AE4798C633323A2684D94A11582BB03A6AB38D2B0D5AE5EABC5E577B`
- Extraction paths used:
  - `EFI\boot\bootx64.efi` -> `assets/efi/bootx64.efi`
  - `assets/efi/grubx64.efi` is rebuilt as standalone GRUB via:
    - `scripts/build-standalone-grub.ps1` (bootstrap config from `$cmdpath`)
    - `grub-mkstandalone (GRUB) 2.12-1ubuntu7.3` (WSL Ubuntu)
- Local bundled file hashes (SHA256):
  - `assets/efi/grubx64.efi` = `0D27908B6F8270E7F3278B044F588F703A9F8363DBB0C18D420232D0E0DD1A0D`
  - `assets/efi/bootx64.efi` = `4C89145E958CF592A6F16552EADF112EF2C1C525E2435C2761E6A99FA88188B3`
  - `assets/efi/checksums.txt` = `268EEF19EBA0282458425D2A7007ACF6850107A36B24EC4349721432F940C7A3`
- Manifest values (CRC32):
  - `grubx64.efi=05A6E630`
  - `bootx64.efi=FAA8033D`
- Signing/trust note:
  - `bootx64.efi` is vendor-signed (`Microsoft Windows UEFI Driver Publisher`).
  - `grubx64.efi` is a local standalone build and is currently unsigned.
  - Secure Boot must remain disabled unless a signed GRUB/shim chain is introduced.

## How to fill missing source metadata

If you still have the source ISO, run:

```powershell
Get-FileHash -Algorithm SHA256 C:\path\to\source.iso
7z l C:\path\to\source.iso | Select-String -Pattern "grubx64.efi|bootx64.efi|shimx64.efi|BOOTx64.EFI"
```

Then replace the `Unknown` fields above with exact values.
