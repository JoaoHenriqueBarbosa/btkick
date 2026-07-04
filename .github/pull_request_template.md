## Summary

<!-- What does this PR change, and why? Link any related issue: Closes #123 -->

## Type of change

<!-- Mark the one that applies (Conventional Commit type). -->

- [ ] `fix` — bug fix
- [ ] `feat` — new feature
- [ ] `docs` — documentation only
- [ ] `refactor` — no behavior change
- [ ] `perf` — performance improvement
- [ ] `test` — adding or fixing tests
- [ ] `chore` / `ci` — tooling, deps, or CI

## How was this tested?

<!--
Describe how you verified the change. Note whether it was tested against real
Bluetooth hardware (and which adapter: built-in vs USB dongle) or only offline.
-->

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo build --release` succeeds
- [ ] `cargo test` passes (add tests if your change is testable)
- [ ] `cargo run -- --render-test` still renders correctly (if the TUI changed)
- [ ] Commits follow [Conventional Commits](https://www.conventionalcommits.org/)
- [ ] Docs/README updated if behavior or usage changed
