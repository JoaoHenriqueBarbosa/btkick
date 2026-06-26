//! Bluetooth primitives + the aggressive connection engine.
//!
//! Everything here shells out to `bluetoothctl` (wrapped in coreutils
//! `timeout` so a hung call can't stall us) and, for the heavy artillery,
//! pokes the dongle's USB node in sysfs to simulate a physical replug.

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const USB_UNBIND: &str = "/sys/bus/usb/drivers/usb/unbind";
const USB_BIND: &str = "/sys/bus/usb/drivers/usb/bind";

/// First `/sys/class/bluetooth/hciN` directory. The index isn't stable — a USB
/// replug can move hci0 → hci1 — so we never hardcode it.
fn hci_dir() -> Option<std::path::PathBuf> {
    let mut hcis: Vec<std::path::PathBuf> = std::fs::read_dir("/sys/class/bluetooth")
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("hci"))
                .unwrap_or(false)
        })
        .collect();
    hcis.sort();
    hcis.into_iter().next()
}

/// Find the USB device node backing the adapter (e.g. "3-1") so we can
/// deauthorize or replug it. Resolves the adapter's `device` symlink and walks
/// up to the first USB device id (has '-', no ':'). Override with $BTKICK_USB_ID.
/// Returns None for built-in/non-USB adapters — the USB tiers are then skipped.
fn usb_node() -> Option<String> {
    if let Ok(v) = std::env::var("BTKICK_USB_ID") {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let resolved = std::fs::canonicalize(hci_dir()?.join("device")).ok()?;
    for anc in resolved.ancestors() {
        if let Some(name) = anc.file_name().and_then(|n| n.to_str()) {
            if name.contains('-')
                && !name.contains(':')
                && std::path::Path::new(&format!("/sys/bus/usb/devices/{name}")).exists()
            {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Progress events streamed out of the connection engine.
pub enum Progress {
    Log(String),
    Connected(f64),
    GaveUp,
}

#[derive(Clone, Default)]
pub struct Device {
    pub mac: String,
    pub name: String,
    pub paired: bool,
    pub bonded: bool,
    pub connected: bool,
    pub trusted: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i32>,
    pub icon: String,
}

#[derive(Clone, Default)]
pub struct Adapter {
    pub mac: String,
    pub name: String,
    pub powered: bool,
    pub discoverable: bool,
    pub pairable: bool,
    pub discovering: bool,
}

// ---- raw command wrappers ---------------------------------------------------

/// Run `bluetoothctl <args>` under a timeout, returning stdout.
pub fn bt(args: &[&str], timeout_s: u32) -> String {
    let mut full = vec![timeout_s.to_string(), "bluetoothctl".to_string()];
    full.extend(args.iter().map(|s| s.to_string()));
    match Command::new("timeout")
        .args(&full)
        .stderr(Stdio::null())
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

pub fn is_connected(mac: &str) -> bool {
    bt(&["info", mac], 4)
        .lines()
        .any(|l| l.trim().starts_with("Connected:") && l.contains("yes"))
}

/// (mac, name) pairs from `bluetoothctl devices`.
fn device_macs() -> Vec<(String, String)> {
    bt(&["devices"], 4)
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ')?;
            Some((mac.to_string(), name.to_string()))
        })
        .collect()
}

pub fn device_info(mac: &str, fallback_name: &str) -> Device {
    let out = bt(&["info", mac], 4);
    let mut d = Device {
        mac: mac.to_string(),
        name: fallback_name.to_string(),
        ..Default::default()
    };
    for line in out.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Name: ") {
            d.name = v.to_string();
        } else if let Some(v) = t.strip_prefix("Icon: ") {
            d.icon = v.to_string();
        } else if let Some(v) = t.strip_prefix("Paired: ") {
            d.paired = v == "yes";
        } else if let Some(v) = t.strip_prefix("Bonded: ") {
            d.bonded = v == "yes";
        } else if let Some(v) = t.strip_prefix("Connected: ") {
            d.connected = v == "yes";
        } else if let Some(v) = t.strip_prefix("Trusted: ") {
            d.trusted = v == "yes";
        } else if let Some(v) = t.strip_prefix("RSSI: ") {
            d.rssi = v.split_whitespace().next().and_then(|s| s.parse().ok());
        } else if let Some(v) = t.strip_prefix("Battery Percentage: ") {
            // Format: "0x61 (97)" — grab the decimal in parens.
            d.battery = v
                .split_once('(')
                .and_then(|(_, rest)| rest.trim_end_matches(')').parse().ok());
        }
    }
    d
}

pub fn list_devices() -> Vec<Device> {
    let mut devs: Vec<Device> = device_macs()
        .into_iter()
        .map(|(mac, name)| device_info(&mac, &name))
        .collect();
    // Connected first, then paired, then by name — stable and useful.
    devs.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    devs
}

pub fn adapter() -> Adapter {
    let out = bt(&["show"], 4);
    let mut a = Adapter::default();
    for line in out.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Controller ") {
            a.mac = v.split_whitespace().next().unwrap_or("").to_string();
        } else if let Some(v) = t.strip_prefix("Name: ") {
            a.name = v.to_string();
        } else if let Some(v) = t.strip_prefix("Powered: ") {
            a.powered = v.starts_with("yes");
        } else if let Some(v) = t.strip_prefix("Discoverable: ") {
            a.discoverable = v.starts_with("yes");
        } else if let Some(v) = t.strip_prefix("Pairable: ") {
            a.pairable = v.starts_with("yes");
        } else if let Some(v) = t.strip_prefix("Discovering: ") {
            a.discovering = v.starts_with("yes");
        }
    }
    a
}

// ---- simple one-shot actions (used by the TUI buttons) ----------------------

pub fn disconnect(mac: &str) {
    bt(&["disconnect", mac], 6);
}
pub fn pair(mac: &str) {
    bt(&["pair", mac], 12);
    bt(&["trust", mac], 3);
}
pub fn remove(mac: &str) {
    bt(&["remove", mac], 6);
}
pub fn set_trust(mac: &str, on: bool) {
    bt(&[if on { "trust" } else { "untrust" }, mac], 3);
}

// ---- the aggressive connection engine --------------------------------------

/// Try *everything* to get `mac` connected, fastest first, escalating to USB
/// replug. Emits `Progress` events and stops the instant it connects or `stop`
/// flips true. Designed to run on its own thread.
pub fn aggressive_connect(mac: String, stop: Arc<AtomicBool>, tx: Sender<Progress>) {
    let start = Instant::now();
    let log = |tx: &Sender<Progress>, m: &str| {
        let _ = tx.send(Progress::Log(m.to_string()));
    };

    if is_connected(&mac) {
        let _ = tx.send(Progress::Connected(0.0));
        return;
    }

    // Background spammer: keep firing `connect` the whole time.
    let spammer = {
        let stop = stop.clone();
        let mac = mac.clone();
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                bt(&["connect", &mac], 6);
                if is_connected(&mac) {
                    stop.store(true, Ordering::Relaxed);
                    return;
                }
                sleep_ms(400);
            }
        })
    };

    // One-time prep to make the stack cooperative.
    bt(&["power", "on"], 4);
    bt(&["agent", "on"], 3);
    bt(&["default-agent"], 3);
    bt(&["trust", &mac], 3);
    log(&tx, "prep: powered on, agent ready, trusted");

    type Tier = fn(&str, &dyn Fn(&str));
    let tiers: &[(&str, Tier)] = &[
        ("nudge: disconnect + reconnect", t_disconnect),
        ("adapter power-cycle", t_power_cycle),
        ("re-pair (remove + scan + pair)", t_repair),
        ("USB deauthorize/reauthorize (soft replug)", t_usb_auth),
        ("USB unbind/bind (hard replug)", t_usb_rebind),
    ];

    let mut round = 0u32;
    while !stop.load(Ordering::Relaxed) {
        round += 1;
        for (name, action) in tiers {
            if stop.load(Ordering::Relaxed) || is_connected(&mac) {
                break;
            }
            log(&tx, &format!("[round {round}] {name}"));
            let emit = |m: &str| log(&tx, &format!("    {m}"));
            action(&mac, &emit);
            if wait_connected(&mac, 4, &stop) {
                break;
            }
        }
    }

    stop.store(true, Ordering::Relaxed);
    let _ = spammer.join();

    if is_connected(&mac) {
        let _ = tx.send(Progress::Connected(start.elapsed().as_secs_f64()));
    } else {
        let _ = tx.send(Progress::GaveUp);
    }
}

// ---- tiers ------------------------------------------------------------------

fn t_disconnect(mac: &str, _emit: &dyn Fn(&str)) {
    bt(&["disconnect", mac], 5);
    sleep_ms(300);
    bt(&["connect", mac], 6);
}

fn t_power_cycle(mac: &str, emit: &dyn Fn(&str)) {
    emit("power off → on");
    bt(&["power", "off"], 4);
    sleep_ms(800);
    bt(&["power", "on"], 4);
    wait_adapter();
    bt(&["trust", mac], 3);
    bt(&["connect", mac], 6);
}

fn t_repair(mac: &str, emit: &dyn Fn(&str)) {
    emit("removing bond, scanning, re-pairing");
    bt(&["remove", mac], 5);
    sleep_ms(300);
    let mut scan = Command::new("bluetoothctl")
        .args(["scan", "on"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok();
    for _ in 0..16 {
        if bt(&["devices"], 4).contains(mac) {
            break;
        }
        sleep_ms(500);
    }
    bt(&["pair", mac], 10);
    bt(&["trust", mac], 3);
    bt(&["connect", mac], 8);
    if let Some(mut s) = scan.take() {
        let _ = s.kill();
        let _ = s.wait();
    }
    bt(&["scan", "off"], 3);
}

fn t_usb_auth(mac: &str, emit: &dyn Fn(&str)) {
    let Some(node) = usb_node() else {
        emit("no USB dongle found (built-in adapter?) — skipping");
        return;
    };
    emit(&format!("deauthorizing USB dongle {node}"));
    write_sysfs(&format!("/sys/bus/usb/devices/{node}/authorized"), "0");
    sleep_ms(1200);
    write_sysfs(&format!("/sys/bus/usb/devices/{node}/authorized"), "1");
    recover_adapter(mac, emit);
}

fn t_usb_rebind(mac: &str, emit: &dyn Fn(&str)) {
    let Some(node) = usb_node() else {
        emit("no USB dongle found (built-in adapter?) — skipping");
        return;
    };
    emit(&format!(
        "unbinding USB dongle {node} from driver (software replug)"
    ));
    write_sysfs(USB_UNBIND, &node);
    sleep_ms(1500);
    write_sysfs(USB_BIND, &node);
    recover_adapter(mac, emit);
}

fn recover_adapter(mac: &str, emit: &dyn Fn(&str)) {
    emit("waiting for adapter to re-enumerate");
    for _ in 0..30 {
        if adapter_present() {
            break;
        }
        sleep_ms(500);
    }
    bt(&["power", "on"], 4);
    wait_adapter();
    bt(&["trust", mac], 3);
    bt(&["connect", mac], 8);
}

// ---- low-level helpers ------------------------------------------------------

fn adapter_present() -> bool {
    hci_dir().is_some() && !bt(&["list"], 4).trim().is_empty()
}

fn wait_adapter() {
    for _ in 0..20 {
        if adapter_present() {
            return;
        }
        sleep_ms(300);
    }
}

fn wait_connected(mac: &str, secs: u32, stop: &Arc<AtomicBool>) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs as u64);
    while Instant::now() < deadline {
        if stop.load(Ordering::Relaxed) || is_connected(mac) {
            stop.store(true, Ordering::Relaxed);
            return true;
        }
        sleep_ms(250);
    }
    false
}

fn write_sysfs(path: &str, val: &str) {
    let child = Command::new("sudo")
        .args(["tee", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if let Ok(mut c) = child {
        if let Some(mut stdin) = c.stdin.take() {
            let _ = stdin.write_all(val.as_bytes());
        }
        let _ = c.wait();
    }
}

fn sleep_ms(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
