# Contributing

Thanks for taking the time to improve PartBoot.

## Development Workflow

1. Create a focused branch for the change.
2. Keep user-facing behavior and documentation in sync.
3. Add or update tests for changed behavior.
4. Run formatting, linting, and tests before submitting.

```powershell
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo +stable-x86_64-pc-windows-gnu test
```

## Documentation

- Keep `README.md` concise and user-facing.
- Put command details and troubleshooting in `docs/usage.md`.
- Put build, release, and implementation details in `DEVELOPMENT.md`.
- Put design records under `docs/architecture/`.
- Put planning notes under `docs/plans/`.

## Safety Expectations

PartBoot touches boot files and EFI partitions. Changes that modify installation, fallback boot paths, partition detection, or generated GRUB entries must include:

- A dry-run path where practical.
- Clear error messages.
- Tests for path handling and guardrails.
- Documentation updates for any changed operator workflow.

## Pull Requests

Include:

- What changed.
- How it was tested.
- Any boot, partition, or EFI risk the reviewer should pay attention to.
