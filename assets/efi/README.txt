Bundled EFI assets directory (Direction 6 packaging).

Expected files in this directory for release artifacts:
- grubx64.efi
- bootx64.efi
- checksums.txt

checksums.txt format:
grubx64.efi=XXXXXXXX
bootx64.efi=YYYYYYYY

Where XXXXXXXX and YYYYYYYY are uppercase CRC32 checksums of each file.

During guided flow (`start`), PartBoot verifies checksums before copying these
files into <root>\cache when cache binaries are missing.
