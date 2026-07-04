# Security Policy

## Supported Versions

btkick is an early-stage project (0.x). Security fixes are applied to the latest
released version on the `main` branch. Please make sure you're on the most recent
release before reporting.

| Version | Supported          |
|---------|--------------------|
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

Instead, email the maintainer directly at **joaohenriquebarbosa21@gmail.com** with:

- A description of the issue and its potential impact.
- Steps to reproduce (a minimal proof of concept is ideal).
- Any relevant environment details (Linux distro, BlueZ version, adapter type).

You can expect an initial acknowledgement within **72 hours**.

## Process

1. **Report received** — we acknowledge your email within 72 hours.
2. **Triage** — we confirm the issue and assess severity and scope.
3. **Fix** — we develop and test a fix on a private branch.
4. **Release** — we publish a patched version and, where appropriate, a security advisory.
5. **Disclosure** — we credit the reporter (unless you prefer to remain anonymous)
   once a fix is available.

## Scope

Reports that are especially relevant to this project include:

- **Memory-safety or `unsafe`-block soundness issues** in the Rust code.
- **Panics reachable from untrusted input** (e.g. malformed `bluetoothctl` output,
  crafted device names, or unexpected sysfs contents that crash the app).
- **Dependency vulnerabilities** in the crates btkick relies on.

Please keep in mind btkick's design when reporting:

- The USB tiers intentionally run `sudo tee` against root-owned `/sys/bus/usb/...`
  nodes to unbind/rebind and (de)authorize the adapter — this is a documented,
  deliberate capability (see the README's *Why `sudo`?* section), not a bug. Reports
  about *unintended* privilege escalation, command injection into those calls, or
  writes to paths outside the intended adapter node are in scope.
- btkick shells out to external programs (`bluetoothctl`, `timeout`, `pactl`). Issues
  arising from attacker-controlled data flowing unsafely into those commands are in
  scope; the mere requirement that those programs be installed is not.

Thank you for helping keep btkick and its users safe.
