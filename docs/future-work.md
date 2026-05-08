# Future Work

Fixable shortcomings captured from the single-disk install workflow:

- Add safer partition guidance and preflight checks so users do not install Linux over the PartBoot storage partition.
- Add explicit warnings when the ISO storage partition and Linux install target appear to be the same partition.
- Improve NTFS UUID discovery so the full NTFS serial is preferred automatically for GRUB and Casper.
- Add a persistent UEFI boot entry installer with `--dry-run`, rollback notes, and no default firmware changes.
- Add Secure Boot support through a documented shim/signing flow instead of relying on disabled Secure Boot.
- Add Windows ISO support through `wimboot` or an extracted Windows installer backend.
- Add tested extracted/live boot profiles for Debian, Kali, Fedora, and Arch instead of filename-only assumptions.
- Add cleanup commands for extracted files, staged EFI files, and fallback EFI files.
- Add installer workflow docs for one-disk systems, including what must not be reformatted during live boot.
