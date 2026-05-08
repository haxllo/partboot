# GRUB Strategy

PartBoot currently uses stock GRUB/shim binaries and generates `grub.cfg`.

PartBoot does not edit or build GRUB from source in this phase. This keeps
Secure Boot, firmware compatibility, filesystem support, and maintenance risk
lower than carrying a custom GRUB fork.

Revisit source-level GRUB customization only when one of these is required:

- embedded static config in the GRUB binary
- custom GRUB modules not available in stock builds
- a full signing pipeline for custom binaries
