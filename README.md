# btkick

[![crates.io](https://img.shields.io/crates/v/btkick.svg)](https://crates.io/crates/btkick)
[![CI](https://github.com/JoaoHenriqueBarbosa/btkick/actions/workflows/ci.yml/badge.svg)](https://github.com/JoaoHenriqueBarbosa/btkick/actions/workflows/ci.yml)
[![license](https://img.shields.io/crates/l/btkick.svg)](./LICENSE)

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

## What it does

When you ask it to connect, `btkick` runs an **escalation ladder**. A background thread hammers `connect` continuously the whole time, while the main thread climbs progressively heavier interventions, re-checking after each step and stopping the instant the device is up:

1. **nudge** — `disconnect` then `connect` (clears a half-open link)
2. **adapter power-cycle** — `power off` → `power on`, then connect
3. **re-pair** — `remove` the bond, scan, `pair`, `trust`, connect
4. **USB deauthorize/reauthorize** — toggle the dongle's `authorized` flag in sysfs (a *soft* replug)
5. **USB unbind/bind** — detach the dongle from its kernel driver and reattach it (a full *software replug* — the same effect as physically unplugging and replugging the USB dongle, no hands required)

Tiers 4–5 only run for USB adapters and are skipped automatically for built-in ones. The adapter's USB node is **auto-detected** from sysfs (no hardcoded paths), and the `hciN` index is never assumed — a replug that moves `hci0` → `hci1` is handled transparently.

It also sets the device **trusted**, which on its own makes BlueZ far more willing to auto-reconnect.

## Install

Runs on Linux with BlueZ (`bluetoothctl`); `sudo` is only needed for the USB-level tiers.

**Prebuilt binary (no toolchain needed)** — fully static musl builds are attached to every [release](https://github.com/JoaoHenriqueBarbosa/btkick/releases):

```sh
curl -fsSL https://github.com/JoaoHenriqueBarbosa/btkick/releases/latest/download/btkick-v0.1.0-x86_64-unknown-linux-musl.tar.gz | tar xz
install -m755 btkick ~/.local/bin/btkick    # or anywhere on your PATH
```

(`aarch64-unknown-linux-musl` is published too.)

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

Press **`f`** on a device to make it the default — after that, bare `btkick` connects straight to it.

## Configuration

- **Default device** is stored as a single line in `~/.config/btkick/default`.
- **USB node override**: if auto-detection picks the wrong device, set `BTKICK_USB_ID` to the sysfs id (e.g. `BTKICK_USB_ID=3-1 btkick`).

## Why `sudo`?

The USB deauthorize and unbind/bind tiers write to root-owned sysfs nodes
(`/sys/bus/usb/...`). Everything else (`bluetoothctl`) runs as your user. If you
don't want passwordless sudo, those two tiers will simply prompt or fail while
the rest of the ladder still runs.

## License

MIT
