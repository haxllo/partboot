# Phase 4: Platform Integration Kickoff

## Goal

Start platform integration work so PartBoot can move from manual firmware selection to managed UEFI entry workflows with explicit safety checks.

## Scope

### In scope
- Add `boot-entry` command family for list/create/remove (dry-run first).
- Improve Windows privilege detection and clear elevation guidance.
- Keep firmware-path boot (`boot-instructions`) as a supported fallback.
- Add guardrails that prevent modifying ambiguous or unsafe targets.

### Out of scope
- Secure Boot signing pipeline redesign.
- New non-UEFI boot backends.
- Cross-platform firmware tooling beyond Windows-first support.

## Initial tasks

1. Define command contract for `boot-entry list|create|remove` with dry-run support.
2. Implement read-only listing first, then guarded creation/removal paths.
3. Add elevated-permission checks and explicit error/help messages.
4. Extend docs with rollback steps for every entry modification path.
5. Add tests for command parsing and no-op safety behavior.

## Phase transition note

Phase 3 delivered reliability upgrades for extraction (dynamic fallback), local ISO extraction cache reuse, and clearer long-running feedback via CLI spinners. Phase 4 now focuses on platform-level UEFI integration while keeping manual fallback paths intact.
