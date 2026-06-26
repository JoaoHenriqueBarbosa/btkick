//! ratatui Bluetooth manager — mouse + keyboard.
//!
//! Layout:
//!   ┌ header: adapter status ───────────────────────────┐
//!   │ device list (60%)        │ detail panel (40%)      │
//!   ├ live log ──────────────────────────────────────────┤
//!   └ clickable button bar ──────────────────────────────┘
//!
//! Connect runs the aggressive engine from `bt.rs` on a background thread and
//! streams its progress into the log pane.

use std::io::{self, Stdout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

use crate::bt::{self, Adapter, Device, Progress};
use crate::config;

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Connect,
    Disconnect,
    Pair,
    Remove,
    ToggleTrust,
    ToggleScan,
    SetDefault,
    Quit,
}

struct Button {
    action: Action,
    // hit box, filled in during draw
    x0: u16,
    x1: u16,
    y: u16,
}

struct App {
    devices: Arc<Mutex<Vec<Device>>>,
    adapter: Arc<Mutex<Adapter>>,
    list_state: ListState,
    default_mac: Option<String>,
    scanning: bool,
    scan_child: Option<std::process::Child>,
    logs: Vec<String>,
    status: String,
    should_quit: bool,

    // connection engine state
    connecting: Option<String>,
    conn_stop: Option<Arc<AtomicBool>>,
    conn_rx: Option<Receiver<Progress>>,

    // mouse hit-testing geometry (filled during draw)
    list_area: Rect,
    buttons: Vec<Button>,

    // refresh thread control
    refresh_stop: Arc<AtomicBool>,
}

impl App {
    fn new() -> Self {
        let devices = Arc::new(Mutex::new(bt::list_devices()));
        let adapter = Arc::new(Mutex::new(bt::adapter()));
        let mut list_state = ListState::default();
        if !devices.lock().unwrap().is_empty() {
            list_state.select(Some(0));
        }
        let refresh_stop = Arc::new(AtomicBool::new(false));
        // Background refresh so bluetoothctl calls never block the UI thread.
        {
            let devices = devices.clone();
            let adapter = adapter.clone();
            let stop = refresh_stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let d = bt::list_devices();
                    let a = bt::adapter();
                    *devices.lock().unwrap() = d;
                    *adapter.lock().unwrap() = a;
                    for _ in 0..12 {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            });
        }
        App {
            devices,
            adapter,
            list_state,
            default_mac: config::read_default(),
            scanning: false,
            scan_child: None,
            logs: vec!["welcome to btkick — Bluetooth manager".into()],
            status: String::new(),
            should_quit: false,
            connecting: None,
            conn_stop: None,
            conn_rx: None,
            list_area: Rect::default(),
            buttons: Vec::new(),
            refresh_stop,
        }
    }

    fn devices_snapshot(&self) -> Vec<Device> {
        self.devices.lock().unwrap().clone()
    }

    fn selected_mac(&self) -> Option<String> {
        let devs = self.devices.lock().unwrap();
        self.list_state
            .selected()
            .and_then(|i| devs.get(i))
            .map(|d| d.mac.clone())
    }

    fn selected_device(&self) -> Option<Device> {
        let devs = self.devices.lock().unwrap();
        self.list_state
            .selected()
            .and_then(|i| devs.get(i))
            .cloned()
    }

    fn log(&mut self, m: impl Into<String>) {
        self.logs.push(m.into());
        if self.logs.len() > 300 {
            let drop = self.logs.len() - 300;
            self.logs.drain(0..drop);
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let len = self.devices.lock().unwrap().len();
        if len == 0 {
            return;
        }
        let cur = self.list_state.selected().unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(len as i32) as usize;
        self.list_state.select(Some(next));
    }

    fn drain_progress(&mut self) {
        let mut finished = false;
        if let Some(rx) = &self.conn_rx {
            while let Ok(p) = rx.try_recv() {
                match p {
                    Progress::Log(m) => self.logs.push(m),
                    Progress::Connected(s) => {
                        self.logs.push(format!("✔ connected in {s:.1}s"));
                        self.status = format!("connected in {s:.1}s");
                        finished = true;
                    }
                    Progress::GaveUp => {
                        self.logs.push("✘ canceled".into());
                        self.status = "canceled".into();
                        finished = true;
                    }
                }
            }
            if self.logs.len() > 300 {
                let drop = self.logs.len() - 300;
                self.logs.drain(0..drop);
            }
        }
        if finished {
            self.connecting = None;
            self.conn_stop = None;
            self.conn_rx = None;
        }
    }

    fn start_connect(&mut self) {
        let Some(mac) = self.selected_mac() else {
            return;
        };
        // If already connecting, the button doubles as Cancel.
        if let Some(stop) = &self.conn_stop {
            stop.store(true, Ordering::Relaxed);
            self.status = "canceling…".into();
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        self.conn_stop = Some(stop.clone());
        self.conn_rx = Some(rx);
        self.connecting = Some(mac.clone());
        self.status = format!("forcing {mac} to connect…");
        self.log(format!("▶ aggressive connect → {mac}"));
        thread::spawn(move || bt::aggressive_connect(mac, stop, tx));
    }

    fn toggle_scan(&mut self) {
        if self.scanning {
            if let Some(mut c) = self.scan_child.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            bt::bt(&["scan", "off"], 3);
            self.scanning = false;
            self.status = "scan off".into();
        } else {
            self.scan_child = std::process::Command::new("bluetoothctl")
                .args(["scan", "on"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok();
            self.scanning = true;
            self.status = "scanning…".into();
        }
    }

    /// Fire-and-forget mutating actions so the UI thread never blocks.
    fn spawn_action(&mut self, action: Action) {
        let Some(mac) = self.selected_mac() else {
            return;
        };
        match action {
            Action::Disconnect => {
                self.status = format!("disconnecting {mac}");
                thread::spawn(move || bt::disconnect(&mac));
            }
            Action::Pair => {
                self.status = format!("pairing {mac}");
                thread::spawn(move || bt::pair(&mac));
            }
            Action::Remove => {
                self.status = format!("removing {mac}");
                thread::spawn(move || bt::remove(&mac));
            }
            Action::ToggleTrust => {
                let trusted = self.selected_device().map(|d| d.trusted).unwrap_or(false);
                self.status = format!("{} {mac}", if trusted { "untrust" } else { "trust" });
                thread::spawn(move || bt::set_trust(&mac, !trusted));
            }
            _ => {}
        }
    }

    fn set_default(&mut self) {
        if let Some(mac) = self.selected_mac() {
            match config::write_default(&mac) {
                Ok(()) => {
                    self.default_mac = Some(mac.clone());
                    self.status = format!("★ default set → {mac}");
                    self.log(format!(
                        "★ default device is now {mac} (btkick connects it directly)"
                    ));
                }
                Err(e) => self.status = format!("could not save default: {e}"),
            }
        }
    }

    fn do_action(&mut self, action: Action) {
        match action {
            Action::Connect => self.start_connect(),
            Action::ToggleScan => self.toggle_scan(),
            Action::SetDefault => self.set_default(),
            Action::Quit => self.should_quit = true,
            Action::Disconnect | Action::Pair | Action::Remove | Action::ToggleTrust => {
                self.spawn_action(action)
            }
        }
    }

    fn on_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char('c') => self.do_action(Action::Connect),
            KeyCode::Char('d') => self.do_action(Action::Disconnect),
            KeyCode::Char('p') => self.do_action(Action::Pair),
            KeyCode::Char('r') => self.do_action(Action::Remove),
            KeyCode::Char('t') => self.do_action(Action::ToggleTrust),
            KeyCode::Char('s') => self.do_action(Action::ToggleScan),
            KeyCode::Char('f') | KeyCode::Char('*') => self.do_action(Action::SetDefault),
            _ => {}
        }
    }

    fn on_mouse(&mut self, kind: MouseEventKind, col: u16, row: u16) {
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Button bar?
                for b in &self.buttons {
                    if row == b.y && col >= b.x0 && col < b.x1 {
                        let a = b.action;
                        self.do_action(a);
                        return;
                    }
                }
                // Device list row?
                let la = self.list_area;
                let inner_top = la.y + 1; // skip top border
                if col > la.x
                    && col < la.x + la.width.saturating_sub(1)
                    && row >= inner_top
                    && row < la.y + la.height.saturating_sub(1)
                {
                    let offset = self.list_state.offset();
                    let idx = offset + (row - inner_top) as usize;
                    if idx < self.devices.lock().unwrap().len() {
                        self.list_state.select(Some(idx));
                    }
                }
            }
            MouseEventKind::ScrollDown => self.move_selection(1),
            MouseEventKind::ScrollUp => self.move_selection(-1),
            _ => {}
        }
    }

    fn cleanup(&mut self) {
        self.refresh_stop.store(true, Ordering::Relaxed);
        if let Some(stop) = &self.conn_stop {
            stop.store(true, Ordering::Relaxed);
        }
        if let Some(mut c) = self.scan_child.take() {
            let _ = c.kill();
            let _ = c.wait();
            bt::bt(&["scan", "off"], 3);
        }
    }
}

impl App {
    /// Build an App with injected state and no background refresh thread —
    /// used by the offline render test (`btkick --render-test`).
    fn test_app() -> Self {
        let devices = vec![
            Device {
                mac: "C0:FF:EE:00:AA:01".into(),
                name: "Wireless Earbuds".into(),
                paired: true,
                bonded: true,
                connected: true,
                trusted: true,
                battery: Some(97),
                rssi: Some(-54),
                icon: "audio-headset".into(),
            },
            Device {
                mac: "C0:FF:EE:00:BB:02".into(),
                name: "Wireless Mouse".into(),
                paired: true,
                trusted: false,
                rssi: Some(-71),
                icon: "input-mouse".into(),
                ..Default::default()
            },
        ];
        let adapter = Adapter {
            mac: "DE:AD:BE:EF:00:00".into(),
            name: "hci0".into(),
            powered: true,
            pairable: true,
            ..Default::default()
        };
        let mut list_state = ListState::default();
        list_state.select(Some(0));
        App {
            devices: Arc::new(Mutex::new(devices)),
            adapter: Arc::new(Mutex::new(adapter)),
            list_state,
            default_mac: Some("C0:FF:EE:00:AA:01".into()),
            scanning: true,
            scan_child: None,
            logs: vec![
                "▶ aggressive connect → C0:FF:EE:00:AA:01".into(),
                "[round 1] adapter power-cycle".into(),
                "✔ connected in 1.7s".into(),
            ],
            status: "connected in 1.7s".into(),
            should_quit: false,
            connecting: None,
            conn_stop: None,
            conn_rx: None,
            list_area: Rect::default(),
            buttons: Vec::new(),
            refresh_stop: Arc::new(AtomicBool::new(true)),
        }
    }
}

/// Render a single frame to a TestBackend and return it as plain text. Lets us
/// verify the layout without a real terminal.
pub fn render_test(width: u16, height: u16) -> String {
    use ratatui::backend::TestBackend;
    let mut app = App::test_app();
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| ui(f, &mut app)).unwrap();
    let buf = term.backend().buffer().clone();
    let mut out = String::new();
    for y in 0..height {
        for x in 0..width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

// ---- rendering --------------------------------------------------------------

fn battery_str(d: &Device) -> String {
    match d.battery {
        Some(b) if b > 0 => format!(" {b}%"),
        _ => String::new(),
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(3),    // body
            Constraint::Length(8), // log
            Constraint::Length(3), // buttons
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(chunks[1]);

    render_device_list(f, app, body[0]);
    render_detail(f, app, body[1]);
    render_log(f, app, chunks[2]);
    render_buttons(f, app, chunks[3]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let a: Adapter = app.adapter.lock().unwrap().clone();
    let on = |b: bool| if b { "on" } else { "off" };
    let dot = |b: bool| if b { Color::Green } else { Color::DarkGray };
    let scan_label = if app.scanning || a.discovering {
        "SCANNING"
    } else {
        "idle"
    };
    let spans = vec![
        Span::styled(" ⬢ ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!(
                "{} ",
                if a.name.is_empty() {
                    "adapter"
                } else {
                    &a.name
                }
            ),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("[{}]  ", a.mac),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw("power:"),
        Span::styled(
            format!("{} ", on(a.powered)),
            Style::default().fg(dot(a.powered)),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{scan_label} "),
            Style::default().fg(if app.scanning || a.discovering {
                Color::Yellow
            } else {
                Color::DarkGray
            }),
        ),
        Span::raw(" default:"),
        Span::styled(
            app.default_mac.clone().unwrap_or_else(|| "none".into()),
            Style::default().fg(Color::Yellow),
        ),
    ];
    let p = Paragraph::new(Line::from(spans))
        .block(Block::default().borders(Borders::ALL).title(" btkick "));
    f.render_widget(p, area);
}

fn render_device_list(f: &mut Frame, app: &mut App, area: Rect) {
    app.list_area = area;
    let devs = app.devices_snapshot();
    let default_mac = app.default_mac.clone();
    let items: Vec<ListItem> = devs
        .iter()
        .map(|d| {
            let is_default = default_mac.as_deref() == Some(d.mac.as_str());
            let conn_dot = if d.connected { "●" } else { "○" };
            let conn_color = if d.connected {
                Color::Green
            } else {
                Color::DarkGray
            };
            let star = if is_default { "★ " } else { "  " };
            let mut spans = vec![
                Span::styled(format!(" {conn_dot} "), Style::default().fg(conn_color)),
                Span::styled(star, Style::default().fg(Color::Yellow)),
                Span::styled(
                    format!("{:<28}", truncate(&d.name, 28)),
                    Style::default().add_modifier(if d.connected {
                        Modifier::BOLD
                    } else {
                        Modifier::empty()
                    }),
                ),
            ];
            let mut tags = String::new();
            if d.paired {
                tags.push_str("paired ");
            }
            if d.trusted {
                tags.push_str("trusted ");
            }
            spans.push(Span::styled(tags, Style::default().fg(Color::Blue)));
            spans.push(Span::styled(
                battery_str(d),
                Style::default().fg(Color::Green),
            ));
            if let Some(r) = d.rssi {
                spans.push(Span::styled(
                    format!(" {r}dBm"),
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = format!(" devices ({}) ", devs.len());
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(
            Style::default()
                .bg(Color::Indexed(238))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▏");
    f.render_stateful_widget(list, area, &mut app.list_state);
}

fn render_detail(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(d) = app.selected_device() {
        let yn = |b: bool| if b { "yes" } else { "no" };
        let kv = |k: &str, v: String, c: Color| {
            Line::from(vec![
                Span::styled(format!("{k:<12}"), Style::default().fg(Color::DarkGray)),
                Span::styled(v, Style::default().fg(c)),
            ])
        };
        lines.push(Line::from(Span::styled(
            d.name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(kv("mac", d.mac.clone(), Color::White));
        if !d.icon.is_empty() {
            lines.push(kv("type", d.icon.clone(), Color::White));
        }
        lines.push(kv(
            "connected",
            yn(d.connected).into(),
            if d.connected {
                Color::Green
            } else {
                Color::Red
            },
        ));
        lines.push(kv("paired", yn(d.paired).into(), Color::White));
        lines.push(kv(
            "trusted",
            yn(d.trusted).into(),
            if d.trusted {
                Color::Green
            } else {
                Color::Yellow
            },
        ));
        if let Some(b) = d.battery {
            if b > 0 {
                lines.push(kv("battery", format!("{b}%"), Color::Green));
            }
        }
        if let Some(r) = d.rssi {
            lines.push(kv("rssi", format!("{r} dBm"), Color::White));
        }
        let is_default = app.default_mac.as_deref() == Some(d.mac.as_str());
        lines.push(kv(
            "default",
            yn(is_default).into(),
            if is_default {
                Color::Yellow
            } else {
                Color::DarkGray
            },
        ));
    } else {
        lines.push(Line::from("no device selected"));
    }
    lines.push(Line::from(""));
    if let Some(mac) = &app.connecting {
        lines.push(Line::from(Span::styled(
            format!("⟳ connecting {mac} (press c/Enter or click Connect to cancel)"),
            Style::default().fg(Color::Yellow),
        )));
    }
    if !app.status.is_empty() {
        lines.push(Line::from(Span::styled(
            app.status.clone(),
            Style::default().fg(Color::Cyan),
        )));
    }
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" detail "))
        .wrap(Wrap { trim: true });
    f.render_widget(p, area);
}

fn render_log(f: &mut Frame, app: &App, area: Rect) {
    let inner_h = area.height.saturating_sub(2) as usize;
    let start = app.logs.len().saturating_sub(inner_h);
    let lines: Vec<Line> = app.logs[start..]
        .iter()
        .map(|l| Line::from(Span::raw(l.clone())))
        .collect();
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" log "));
    f.render_widget(p, area);
}

fn render_buttons(f: &mut Frame, app: &mut App, area: Rect) {
    let connect_label = if app.connecting.is_some() {
        "[ Cancel ]"
    } else {
        "[ Connect ]"
    };
    let scan_label = if app.scanning {
        "[ Scan✓ ]"
    } else {
        "[ Scan ]"
    };
    let specs: [(&str, Action); 8] = [
        (connect_label, Action::Connect),
        ("[ Disconnect ]", Action::Disconnect),
        ("[ Pair ]", Action::Pair),
        ("[ Remove ]", Action::Remove),
        ("[ Trust ]", Action::ToggleTrust),
        (scan_label, Action::ToggleScan),
        ("[ ★Default ]", Action::SetDefault),
        ("[ Quit ]", Action::Quit),
    ];

    app.buttons.clear();
    let mut spans: Vec<Span> = Vec::new();
    let y = area.y + 1; // inside top border
    let mut x = area.x + 1; // inside left border
    for (label, action) in specs {
        let w = label.chars().count() as u16;
        app.buttons.push(Button {
            action,
            x0: x,
            x1: x + w,
            y,
        });
        let color = match action {
            Action::Connect if app.connecting.is_some() => Color::Red,
            Action::Connect => Color::Green,
            Action::Quit => Color::Red,
            Action::SetDefault => Color::Yellow,
            _ => Color::White,
        };
        spans.push(Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        x += w + 1;
    }
    let p = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" actions — click or use keys: c/Enter d p r t s f q "),
    );
    f.render_widget(p, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

// ---- entry point ------------------------------------------------------------

pub fn run() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal: Terminal<CrosstermBackend<Stdout>> = Terminal::new(backend)?;

    let mut app = App::new();
    let res = event_loop(&mut terminal, &mut app);

    app.cleanup();
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    res
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        app.drain_progress();
        // Keep selection in range as the list refreshes underneath us.
        let len = app.devices.lock().unwrap().len();
        if len == 0 {
            app.list_state.select(None);
        } else if app.list_state.selected().map(|i| i >= len).unwrap_or(true) {
            app.list_state.select(Some(len - 1));
        }

        terminal.draw(|f| ui(f, app))?;

        if event::poll(Duration::from_millis(150))? {
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k.code),
                Event::Mouse(m) => app.on_mouse(m.kind, m.column, m.row),
                _ => {}
            }
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
