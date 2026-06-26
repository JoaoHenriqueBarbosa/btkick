//! btkick — aggressive Bluetooth connector + full TUI manager.
//!
//!   btkick              connect the configured default device (aggressively)
//!   btkick <MAC>        connect a specific device
//!   btkick -d [MAC]     disconnect the default (or given) device
//!   btkick tui          open the interactive manager (mouse + keyboard)
//!   btkick -h           help

mod bt;
mod config;
mod tui;

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Flags / subcommands first.
    if args.iter().any(|a| a == "-h" || a == "--help") {
        help();
        return;
    }
    if args.iter().any(|a| a == "--render-test") {
        print!("{}", tui::render_test(100, 26));
        return;
    }
    if args.iter().any(|a| a == "tui" || a == "-t" || a == "--tui") {
        if let Err(e) = tui::run() {
            eprintln!("tui error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let disconnect = args.iter().any(|a| a == "-d" || a == "--disconnect");
    // Any non-flag argument is treated as a MAC override.
    let mac_arg = args
        .iter()
        .find(|a| !a.starts_with('-') && a.contains(':'))
        .cloned();

    let mac = match mac_arg.or_else(config::read_default) {
        Some(m) => m,
        None => {
            eprintln!(
                "no default device set. run `btkick tui` and press f on a device to set one,\nor pass a MAC: btkick AA:BB:CC:DD:EE:FF"
            );
            std::process::exit(2);
        }
    };

    if disconnect {
        println!("btkick → disconnecting {mac}");
        bt::disconnect(&mac);
        if bt::is_connected(&mac) {
            eprintln!("✘ still connected");
            std::process::exit(1);
        }
        println!("✔ disconnected");
        return;
    }

    connect_cli(&mac);
}

/// CLI connect: run the engine on a thread and print its progress as it streams.
fn connect_cli(mac: &str) {
    println!("btkick → forcing {mac} to connect (Ctrl-C to abort)\n");
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let mac_owned = mac.to_string();
    let handle = thread::spawn(move || bt::aggressive_connect(mac_owned, stop, tx));

    for p in rx {
        match p {
            bt::Progress::Log(m) => println!("{m}"),
            bt::Progress::Connected(s) => {
                println!("\n✔ connected in {s:.1}s");
                break;
            }
            bt::Progress::GaveUp => {
                println!("\n✘ gave up");
                break;
            }
        }
    }
    let _ = handle.join();
}

fn help() {
    println!(
        "btkick — aggressive Bluetooth connector + TUI manager\n\n\
         usage:\n  \
         btkick              connect the default device (aggressive engine)\n  \
         btkick <MAC>        connect a specific device\n  \
         btkick -d [MAC]     disconnect default (or given) device\n  \
         btkick tui          open the interactive manager (mouse + keyboard)\n  \
         btkick -h           this help\n\n\
         TUI keys: ↑/↓ or j/k move · c/Enter connect · d disconnect · p pair\n            \
         r remove · t trust · s scan · f set default · q quit (clicks work too)"
    );
}
