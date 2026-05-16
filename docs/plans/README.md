# Planning Status

This index tracks implementation status for each plan document as of `2026-05-16`.

## Status Key

- `Done`: implemented in code and covered by current tests.
- `Partial`: some tasks implemented; others pending, operational-only, or superseded.
- `Open`: planned but not implemented yet.
- `Superseded`: replaced by a later direction or implementation.

## Plans

| Plan | Status | Notes |
| --- | --- | --- |
| `2026-05-08-partboot-mvp.md` | Done | Baseline CLI/layout/scanner/menu foundation is in place and tests pass. |
| `2026-05-07-shutdown-loop-fix.md` | Partial | Ubuntu `toram noprompt` behavior implemented; extracted-first follow-up was superseded. |
| `2026-05-08-tailored-grub-menu.md` | Partial / Superseded | Diagnostics survived; branded header/labels were replaced by clean-menu direction. |
| `2026-05-08-clean-menu-profiles.md` | Partial | Diagnostics flag, doctor checks, and architecture doc done; profile-driven menu behavior still pending. |
| `2026-05-08-extracted-casper.md` | Partial | Extraction pipeline exists; GRUB does not yet prefer extracted boot entries. |
| `2026-05-08-12-directions-and-ux-plan.md` | Partial | Direction 1 substantially complete; later directions are mixed partial/open. |
| `2026-05-08-direction-6-release-packaging.md` | Partial | Bundling/checksum/release scripts implemented; clean-machine validation still pending. |
| `2026-05-12-phase-4-platform-integration.md` | Done | `entry` command family implemented with dry-run, backups, Secure Boot detection, rollback docs, and 30+ tests. |
