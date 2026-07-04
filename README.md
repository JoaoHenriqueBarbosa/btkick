# btkick

[![crates.io](https://img.shields.io/crates/v/btkick.svg)](https://crates.io/crates/btkick)
[![CI](https://github.com/JoaoHenriqueBarbosa/btkick/actions/workflows/ci.yml/badge.svg)](https://github.com/JoaoHenriqueBarbosa/btkick/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/btkick.svg)](./LICENSE)
[![lines of code](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/JoaoHenriqueBarbosa/btkick/main/.github/badges/loc.json)](https://github.com/JoaoHenriqueBarbosa/btkick)

**Kick flaky Bluetooth into connecting — fast.** A Rust CLI + terminal UI for Linux/BlueZ that, when you hit *Connect*, aggressively tries *everything* (and some things you wouldn't bother to) until the device is up, in the shortest time possible.

If you've ever had a Bluetooth headset, mouse or controller that connects on the first try *sometimes*, and other times needs a `power off`/`power on`, an unpair/re-pair, or you physically yanking the USB dongle out and back in — that flakiness is a well-known BlueZ/kernel behavior, and it's not specific to any one device. `btkick` automates the whole "try this, then that, then the nuclear option" dance.

```
┌ btkick ──────────────────────────────────────────────────────────────────────────────────────────┐
│ ⬢ hci0 [DE:AD:BE:EF:00:00]  power:on  SCANNING  default:C0:FF:EE:00:AA:01                          │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ devices (2) ───────────────────────────────────────────┐┌ detail ────────────────────────────────┐
│▏ ● ★ Wireless Earbuds            paired trusted  97% -5││Wireless Earbuds                        │
│  ○   Wireless Mouse              paired  -71dBm        ││mac         C0:FF:EE:00:AA:01           │
│                                                        ││type        audio-headset               │
│                                                        ││connected   yes                         │
│                                                        ││paired      yes                         │
│                                                        ││trusted     yes                         │
│                                                        ││battery     97%                         │
│                                                        ││rssi        -54 dBm                     │
│                                                        ││default     yes                         │
└────────────────────────────────────────────────────────┘└────────────────────────────────────────┘
┌ log ─────────────────────────────────────────────────────────────────────────────────────────────┐
│▶ aggressive connect → C0:FF:EE:00:AA:01                                                            │
│[round 1] adapter power-cycle                                                                       │
│✔ connected in 1.7s                                                                                 │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ actions — click or use keys: c/Enter d p r t s f q ──────────────────────────────────────────────┐
│[ Connect ] [ Disconnect ] [ Pair ] [ Remove ] [ Trust ] [ Scan✓ ] [ ★Default ] [ Quit ]           │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
```

> The frame above is exactly what `btkick --render-test` prints — a single TUI frame rendered offline, with no hardware or Bluetooth services required.

## Highlights

- **Escalation ladder with a concurrent spammer.** One background thread hammers `connect` non-stop while the main thread climbs five progressively heavier interventions, re-checking after each and stopping the *instant* the device is up. Coordinated with a shared `AtomicBool` for immediate cancellation. Plain `std::thread` + `mpsc` — no async runtime.
- **A real "nuclear option."** For USB dongles, the last two tiers simulate a physical replug in software: toggling the `authorized` sysfs flag, then unbinding/rebinding the kernel driver. No hands required.
- **No hardcoded paths.** The adapter's USB node is auto-detected by walking the sysfs ancestors of the adapter's `device` symlink, and the `hciN` index is never assumed — a replug that moves `hci0` → `hci1` is handled transparently.
- **Can't hang.** *Every* `bluetoothctl` invocation runs under coreutils `timeout`, so a stuck call can never stall the app.
- **Audio follows the device.** On connect it moves the default audio sink (PipeWire/PulseAudio via `pactl`) to the device and remembers the previous sink on disk; on disconnect it restores that *exact* sink (with a local-output fallback). Best-effort — a no-op if `pactl` isn't present.
- **Non-blocking TUI.** A background refresh thread keeps the device list and adapter state current, and mutating actions are fire-and-forget on their own threads, so the UI never freezes. Mouse hit-testing (buttons and rows) is done against geometry captured during draw.

## Requirements

**Platform: Linux only.** The prebuilt binaries are statically linked (musl) and run on any Linux without a toolchain, but the *functionality* depends on these external programs, which `btkick` shells out to. It does **not** install or check for them — a missing program means that capability silently no-ops.

| Program | Package | Needed for | Required? |
|---------|---------|------------|-----------|
| `bluetoothctl` | BlueZ | every Bluetooth operation | **Required** |
| `timeout` | GNU coreutils | wraps every `bluetoothctl` call (hang protection) | **Required** |
| `pactl` | PipeWire / PulseAudio | audio routing on connect/disconnect | Optional |
| `sudo` + `tee` | sudo, coreutils | USB tiers 4–5 only (sysfs writes) | Optional |

> Without coreutils `timeout` on your `PATH`, Bluetooth operations become silent no-ops — every `bluetoothctl` call is launched *through* `timeout`.

## Install

**Download a ready-to-run binary (no Rust needed)** — grab the file for your CPU from the [latest release](https://github.com/JoaoHenriqueBarbosa/btkick/releases/latest):

- `...-x86_64-...` for a normal PC/laptop (Intel/AMD)
- `...-aarch64-...` for ARM (e.g. a Raspberry Pi)

It's a single self-contained file that runs on any Linux. Unzip it and put it on your PATH:

```sh
tar xzf btkick-*-x86_64-unknown-linux-musl.tar.gz   # the file you downloaded
install -m755 btkick ~/.local/bin/btkick            # now you can run `btkick`
```

**From crates.io:**

```sh
cargo install btkick
```

**From source:**

```sh
git clone https://github.com/JoaoHenriqueBarbosa/btkick
cd btkick
cargo build --release
ln -sf "$PWD/target/release/btkick" ~/.local/bin/btkick
```

## Usage

```sh
btkick                 # connect the configured default device (aggressive engine)
btkick <MAC>           # connect a specific device, e.g. btkick 40:35:E6:21:BF:7F
btkick -d [MAC]        # disconnect the default (or given) device
btkick tui             # open the interactive manager (mouse + keyboard)
btkick -h              # help
```

The typical flow: run `btkick tui`, press `f` on a device to make it the **default**, then bare `btkick` connects straight to it from then on.

### What the connect engine does

When you ask it to connect, `btkick` runs an **escalation ladder**. A background thread hammers `connect` continuously the whole time, while the main thread climbs progressively heavier interventions, re-checking after each step and stopping the instant the device is up:

1. **nudge** — `disconnect` then `connect` (clears a half-open link)
2. **adapter power-cycle** — `power off` → `power on`, then connect
3. **re-pair** — `remove` the bond, scan, `pair`, `trust`, connect
4. **USB deauthorize/reauthorize** — toggle the dongle's `authorized` flag in sysfs (a *soft* replug)
5. **USB unbind/bind** — detach the dongle from its kernel driver and reattach it (a full *software replug* — the same effect as physically unplugging and replugging the USB dongle, no hands required)

Tiers 4–5 only run for USB adapters and are skipped automatically for built-in ones. It also sets the device **trusted**, which on its own makes BlueZ far more willing to auto-reconnect.

**Audio follows the device.** Connecting at the BlueZ level isn't enough — the sound has to actually move. On connect, btkick switches the default audio output (via `pactl`) to the device and moves any playing streams onto it, remembering whichever sink was active before. On disconnect it restores that exact sink (falling back to a local output if it's gone). This is best-effort and relies on `bluez_output.*` sink names plus an analog/HDMI heuristic, so it's not guaranteed to pick the perfect sink in every setup.

### TUI

The TUI works with **both mouse clicks and the keyboard**.

| key | action | | key | action |
|-----|--------|-|-----|--------|
| `↑`/`↓` or `j`/`k` | move selection | | `t` | toggle trust |
| `c` / `Enter` | aggressive connect (again to cancel) | | `s` | toggle scan |
| `d` | disconnect | | `f` / `*` | set as **default** |
| `p` | pair | | `q` / `Esc` | quit |
| `r` | remove (unpair) | | | |

Click a device row to select it, click a button to run its action, scroll to navigate.

## Configuration

- **Default device** is stored as a single line in `~/.config/btkick/default`.
- **Previous audio sink** is remembered in `~/.config/btkick/prev_sink` so it can be restored on disconnect.
- **USB node override**: if auto-detection picks the wrong device, set `BTKICK_USB_ID` to the sysfs id (e.g. `BTKICK_USB_ID=3-1 btkick`).

## Why `sudo`?

The USB deauthorize and unbind/bind tiers write to root-owned sysfs nodes
(`/sys/bus/usb/...`) — they detach the adapter from its kernel driver and toggle
its authorization, which has the same effect as a physical replug of the dongle.
Everything else (`bluetoothctl`) runs as your user. If you don't want passwordless
sudo, those two tiers will simply prompt or fail while the rest of the ladder still
runs.

## Development

```sh
cargo build --release        # optimized build (LTO); produces target/release/btkick
cargo build                  # debug build
cargo run -- tui             # run the TUI from source
cargo run -- --render-test   # print one TUI frame as plain text, offline (no hardware)
```

Before opening a PR, run what CI enforces:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

> **On testing:** this is an early-stage tool (v0.1.x) with **no automated test suite yet** — `cargo test` currently runs zero tests. There *is* a `--render-test` command that renders a full TUI frame offline against ratatui's `TestBackend` and prints it, which is handy for eyeballing the UI without a terminal, but it is not wired up as an automated assertion. The green CI badge reflects `fmt` + `clippy` + a successful build, not behavioral coverage. Contributions that add real tests are very welcome.

## Architecture

Single binary, four modules, two direct dependencies (`ratatui`, `crossterm`). No async runtime, no `serde`.

```
btkick
├── Cargo.toml          # package metadata, 2 deps, release profile (LTO)
├── Cargo.lock          # 70 packages total
├── LICENSE             # MIT
├── .github/workflows/  # ci (fmt/clippy/build), release, release-plz
└── src/
    ├── main.rs         # CLI dispatch, streaming connect, help
    ├── bt.rs           # bluetoothctl wrappers, aggressive engine (5 tiers), pactl audio, sysfs USB
    ├── config.rs       # persist default MAC + previous sink under ~/.config/btkick/
    └── tui.rs          # ratatui app: device list, detail, live log, buttons, mouse + keyboard
```

Prebuilt release binaries (static musl, x86_64 and aarch64) are produced by a reusable GitHub Actions workflow and attached to each GitHub release; releases are cut with [release-plz](https://github.com/release-plz/release-plz).

## Contributing

Bug reports, feature ideas and PRs are welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md) and the [Code of Conduct](./CODE_OF_CONDUCT.md). Security issues: please follow [SECURITY.md](./SECURITY.md).

## License

Licensed under the [MIT License](./LICENSE).
