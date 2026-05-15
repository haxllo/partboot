# PartBoot Improvement Directions (12) + Direction 1 Plan

## Status Audit (2026-05-15)

Overall status: `Partial`

Direction status snapshot:

1. `UX & CLI ergonomics`: `Mostly done` (guided flow, `start`, standardized output, `--json` on key commands).
2. `Safety & guardrails`: `Partial` (install force/dry-run and UUID checks exist; broader target-risk confirmations still open).
3. `Boot compatibility & profiles`: `Partial` (family coverage improved; profile-driven menu behavior still incomplete).
4. `Platform integration`: `Open/early` (no dedicated `boot-entry list|create|remove` command family yet).
5. `Reliability, testing, and operations`: `Partial` (unit coverage is strong; end-to-end integration tests still open).
6. `Release & packaging`: `Partial` (bundling/checksum/provenance automation implemented; final clean-machine validation pending).
7. `Observability`: `Partial` (helpful runtime output exists; structured diagnostics bundle/export still open).
8. `Performance`: `Partial` (cache reuse and extraction fallback improvements present; measured baselines still open).
9. `Extensibility`: `Open` (extension contracts/generation tooling not yet formalized).
10. `Recovery tooling`: `Partial` (manual fallback/install helpers exist; one-command restore/cleanup still open).
11. `Documentation quality`: `Partial` (core docs improved; deeper decision trees/playbooks still open).
12. `Governance & security`: `Open/partial` (checksum/provenance checks exist, broader security governance still open).

## Scope control rule (must follow)

We will not expand scope beyond the 12 directions below until work planned under those directions is completed in order.  
No new direction is added unless one of these 12 is explicitly completed or replaced.

---

## 12 directions (detailed)

1. **UX & CLI ergonomics**
   - Add guided flows for common tasks (init → scan → extract → generate-menu → stage/install).
   - Improve output readability (clear statuses, warnings, next-step hints).
   - Add machine-friendly output mode (`--json`) for scripting/automation.
   - Standardize command help text and examples around real workflows.

2. **Safety & guardrails**
   - Detect risky target combinations (wrong ESP, wrong data partition).
   - Add explicit confirmations for destructive/overwriting operations.
   - Provide backup and rollback guidance before write operations.
   - Prevent ambiguous runs when required safety context is missing.

3. **Boot compatibility & profiles**
   - Strengthen profile-driven boot behavior, avoid filename-only assumptions.
   - Expand validated profiles for Debian, Kali, Fedora, Arch.
   - Improve profile schema validation and profile diagnostics.
   - Keep fallback strategy explicit and predictable per ISO.

4. **Platform integration**
   - Add persistent UEFI boot-entry management (create/list/remove).
   - Improve Windows-specific privilege and environment checks.
   - Better interoperability with firmware variations and boot menus.
   - Keep manual and automated boot paths both supported.

5. **Reliability, testing, and operations**
   - Add integration tests for full workflow scenarios.
   - Improve deterministic output checks for generated GRUB config.
   - Add verification and repair commands for installed layouts.
   - Raise confidence in repeatability across clean systems.

6. **Release & packaging**
   - Provide packaged releases (zip/installer) for non-dev usage.
   - Bundle required EFI boot binaries (`grubx64.efi`, `bootx64.efi`) in releases so `start` works without manual cache setup (accept ~3-6 MB package growth).
   - Add signed artifacts and versioned release notes.
   - Define upgrade compatibility policy across profile/config changes.
   - Provide stable channels for early adopters vs tested releases.

7. **Observability**
   - Structured logs with error codes and diagnostic context.
   - Support diagnostic bundle export for troubleshooting.
   - Capture run summaries suitable for issue reports.
   - Keep logging helpful without exposing sensitive system details.

8. **Performance**
   - Speed up scan/extract/menu generation paths.
   - Cache expensive checks and avoid redundant work.
   - Improve large-ISO handling and repeated-run performance.
   - Measure and report performance baselines before optimization.

9. **Extensibility**
   - Define extension points for new boot backends and profiles.
   - Support template-based profile generation.
   - Keep core behavior stable while enabling optional extensions.
   - Document extension contracts and compatibility expectations.

10. **Recovery tooling**
    - Add one-command restore for known-good ESP/fallback state.
    - Provide safe cleanup commands for staged/extracted artifacts.
    - Include quick recovery playbooks for common failure modes.
    - Minimize manual repair steps after failed experiments.

11. **Documentation quality**
    - Add end-to-end playbooks for single-disk and test-disk workflows.
    - Add troubleshooting decision trees and symptom-based guidance.
    - Keep examples aligned with real command outputs.
    - Ensure docs reflect actual command behavior and defaults.

12. **Governance & security**
    - Document threat model and trust boundaries.
    - Improve signing, verification, and supply-chain hygiene.
    - Keep security-sensitive operations explicit and auditable.
    - Add security-focused review checklist for new features.

---

## Active plan: Direction 1 — UX & CLI ergonomics

### Goal
Make PartBoot easier to run correctly on first attempt, while remaining scriptable for advanced users.

### In-scope (Direction 1 only)
- Guided high-level workflow command.
- Cleaner human-readable command output.
- `--json` output for key commands.
- Consistent help text and examples.

### Out-of-scope for this phase
- New boot backends.
- Secure Boot/signing pipeline changes.
- Persistent UEFI boot entry creation.
- Deep profile compatibility expansion beyond current supported flow.

### Execution plan

1. **Design UX contract**
   - Define shared output format (human + json) and message severity levels.
   - Define a consistent command-success and command-failure shape.

2. **Add guided command**
   - Introduce a guided workflow command that runs key steps in sequence with checks.
   - Include clear stop conditions and actionable error messages.

3. **Standardize output**
   - Refactor existing command outputs to consistent labels and step markers.
   - Ensure warnings are explicit when fallback assumptions are used.

4. **Add `--json` mode**
   - Implement for `scan`, `generate-menu`, and `doctor` first.
   - Keep stable field names to support automation.

5. **Update docs and examples**
   - Add one canonical “quick path” and one “safe step-by-step path.”
   - Align examples with `H:\partboot` + `S:\` test workflow.

### Completion criteria
- Guided command can run the common test workflow end-to-end.
- Human-readable output is consistent and concise.
- `--json` output exists and is stable for selected commands.
- README and command help reflect the new UX behavior.

---

## Next plan: Direction 6 — Release & packaging

### Why this is next
- Direction 1 UX flow is complete.
- Main remaining UX friction is manual cache bootstrap for EFI binaries.
- Bundling solves first-run reliability with small package size impact.

### Hardcoded/runtime findings carried into this direction
- Runtime version string should come from build metadata (fixed).
- Boot-instructions should not hardcode a specific ISO name (fixed).
- 7-Zip absolute install path should not be hardcoded; use PATH/configurable override (fixed via `PARTBOOT_7Z_PATH`).

### In-scope tasks
1. Bundle `grubx64.efi` and `bootx64.efi` with release assets.
2. Add release packaging layout that drops bundled EFI binaries into `cache` on first run.
3. Add checksum metadata and verification step for bundled binaries.
4. Add release notes section documenting source/version/signing status of bundled EFI binaries.
5. Keep fallback behavior: if bundled binaries are missing, show explicit recovery command/output.

### Out-of-scope for this direction
- Custom GRUB source fork/build.
- Persistent UEFI entry creation automation.
- Secure Boot signing pipeline redesign.

### Completion criteria
- Fresh install can run `start` without manual `cache` file preparation.
- Release artifact includes and verifies required EFI binaries.
- Documentation clearly states bundled binary provenance and fallback behavior.
