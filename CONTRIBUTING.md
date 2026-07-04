# Contributing to btkick

Thanks for your interest in improving **btkick**! Bug reports, feature ideas, docs
fixes and pull requests are all welcome. This is a small, early-stage Rust project,
so the process is deliberately lightweight.

By participating you agree to abide by our [Code of Conduct](./CODE_OF_CONDUCT.md).

## Prerequisites

- **Rust** (stable). The crate's minimum supported version (MSRV) is **1.74**, but the
  latest stable toolchain is recommended. Install via [rustup](https://rustup.rs/).
- **Linux** with **BlueZ** (`bluetoothctl`) and GNU **coreutils** (`timeout`) to run
  against real hardware. See the [Requirements](./README.md#requirements) table.
- Optional: `pactl` (PipeWire/PulseAudio) if you're touching the audio routing paths.

You don't need Bluetooth hardware to build the project or to work on the TUI — use
`cargo run -- --render-test` to render a frame offline.

## Getting set up

```sh
# 1. Fork the repo on GitHub, then clone your fork
git clone https://github.com/<your-username>/btkick
cd btkick

# 2. Build it
cargo build --release

# 3. Try it without hardware
cargo run -- --render-test
cargo run -- -h
```

## Repository layout

```
src/
├── main.rs     # CLI dispatch, streaming connect, help
├── bt.rs       # bluetoothctl wrappers, the 5-tier aggressive engine, pactl audio, sysfs USB
├── config.rs   # persist default MAC + previous audio sink under ~/.config/btkick/
└── tui.rs      # ratatui app: device list, detail, live log, clickable buttons, mouse + keyboard
.github/
└── workflows/  # ci.yml (fmt/clippy/build), release.yml, release-plz.yml
```

There is a single binary target (`btkick`), no library. The connect engine lives in
`bt::aggressive_connect`; the TUI entry point is `tui::run`.

## Development workflow

1. **Fork** the repository and create a topic branch off `main`:
   ```sh
   git checkout -b feat/short-description
   ```
2. **Make your change.** Keep it focused — one logical change per PR.
3. **Run the checks below** and make sure they pass.
4. **Commit** using Conventional Commits (see the table below).
5. **Push** to your fork and **open a pull request** against `main`. Fill in the
   PR template, describing what changed and how you tested it.

CHANGELOG entries and version bumps are handled automatically by
[release-plz](https://github.com/release-plz/release-plz) from your commit
messages — you don't need to edit `CHANGELOG.md` or `Cargo.toml`'s version by hand.

## Checks to run before you push

These are exactly what CI enforces, so running them locally avoids a red build:

```sh
cargo fmt --all --check                       # formatting
cargo clippy --all-targets -- -D warnings     # lints (warnings are errors)
cargo build --release                         # it compiles
cargo test                                     # currently a no-op — see below
```

> **Heads up on tests:** there is no automated test suite yet, so `cargo test` runs
> zero tests today. If your change is testable, **please add tests** — new coverage
> is one of the most valuable contributions this project can receive. The
> `--render-test` command (offline TUI rendering via ratatui's `TestBackend`) is a
> good starting point for snapshot-style UI tests.

## Conventional Commits

Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/).
The type drives the automated changelog and the next semantic version.

| Type       | Use for                                                        | Version bump |
|------------|----------------------------------------------------------------|--------------|
| `feat`     | a new user-facing feature                                      | minor        |
| `fix`      | a bug fix                                                       | patch        |
| `docs`     | documentation only                                             | none         |
| `refactor` | code change that neither fixes a bug nor adds a feature        | none         |
| `perf`     | a performance improvement                                      | patch        |
| `test`     | adding or fixing tests                                         | none         |
| `chore`    | tooling, deps, metadata, CI plumbing                           | none         |
| `ci`       | changes to CI/CD workflows                                     | none         |
| `style`    | formatting/whitespace with no code-behavior change             | none         |

A `!` after the type (e.g. `feat!:`) or a `BREAKING CHANGE:` footer signals a breaking
change (major bump). Example:

```
feat(engine): add a configurable per-tier timeout

Lets the caller cap how long each escalation tier runs before moving on.
```

## Reporting bugs and requesting features

Use the issue templates:

- **Bug report** — include your Linux distro, BlueZ version, the adapter (built-in vs
  USB dongle), and the log output from the TUI or CLI.
- **Feature request** — describe the problem you're trying to solve first.

For anything security-sensitive, do **not** open a public issue — follow
[SECURITY.md](./SECURITY.md) instead.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](./LICENSE) that covers this project.
