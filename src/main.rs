//! netwatch — a menu bar / system tray indicator for internet connectivity.
//!
//! Polls Apple's captive-portal probe. When the connection is down the icon
//! throbs; when a captive portal is detected it also opens a login window
//! (the Captive Network Assistant on macOS, the browser on Windows).

// No console window in release builds; `win::attach_console` restores CLI output.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(target_os = "macos")]
mod launch_agent;
#[cfg(target_os = "macos")]
use launch_agent as autostart;
#[cfg(windows)]
mod run_key;
#[cfg(windows)]
use run_key as autostart;
#[cfg(windows)]
mod win;

use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const PROBE_URL: &str = "http://captive.apple.com/hotspot-detect.html";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_ONLINE: Duration = Duration::from_secs(10);
const POLL_OFFLINE: Duration = Duration::from_secs(3);
#[cfg(target_os = "macos")]
const PORTAL_FALLBACK_URL: &str = "http://captive.apple.com";
/// The page Windows itself opens when it detects a captive portal.
#[cfg(windows)]
const PORTAL_FALLBACK_URL: &str = "http://www.msftconnecttest.com/redirect";
#[cfg(target_os = "macos")]
const CAPTIVE_ASSISTANT: &str = "/System/Library/CoreServices/Captive Network Assistant.app";

const ICON_SIZE: u32 = 36;
const THROB_FRAMES: usize = 24;
const FRAME_INTERVAL: Duration = Duration::from_millis(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Unknown,
    Online,
    Captive,
    Offline,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Unknown => "Checking connection…",
            Status::Online => "Connected",
            Status::Captive => "Captive portal — login required",
            Status::Offline => "No internet connection",
        }
    }

    fn throbs(self) -> bool {
        matches!(self, Status::Captive | Status::Offline)
    }
}

#[derive(Debug)]
enum UserEvent {
    Status(Status),
    Menu(MenuEvent),
}

fn probe(agent: &ureq::Agent, url: &str) -> Status {
    match agent.get(url).call() {
        Ok(mut resp) => {
            let ok = resp.status() == 200
                && resp
                    .body_mut()
                    .read_to_string()
                    .is_ok_and(|body| body.contains("<TITLE>Success</TITLE>"));
            if ok { Status::Online } else { Status::Captive }
        }
        Err(_) => Status::Offline,
    }
}

/// Runs the probe loop on a background thread. Sending on the returned
/// channel triggers an immediate re-check.
fn spawn_checker(proxy: EventLoopProxy<UserEvent>) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    thread::spawn(move || {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(PROBE_TIMEOUT))
            .max_redirects(0)
            .http_status_as_error(false)
            .build()
            .into();
        let url = std::env::var("NETWATCH_PROBE_URL").unwrap_or_else(|_| PROBE_URL.to_string());
        loop {
            let status = probe(&agent, &url);
            if proxy.send_event(UserEvent::Status(status)).is_err() {
                return;
            }
            let wait = if status == Status::Online {
                POLL_ONLINE
            } else {
                POLL_OFFLINE
            };
            match rx.recv_timeout(wait) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
    });
    tx
}

/// Opens the Captive Network Assistant. It quits straight away if macOS hasn't
/// flagged the network as captive itself, so fall back to the browser, where the
/// portal will intercept the probe page.
#[cfg(target_os = "macos")]
fn open_login_window() {
    thread::spawn(|| {
        let _ = Command::new("open").arg(CAPTIVE_ASSISTANT).status();
        thread::sleep(Duration::from_secs(2));
        let running = Command::new("pgrep")
            .args(["-x", "Captive Network Assistant"])
            .status()
            .is_ok_and(|s| s.success());
        if !running {
            let _ = Command::new("open").arg(PORTAL_FALLBACK_URL).status();
        }
    });
}

/// Opens Microsoft's captive-portal redirect page in the default browser,
/// where the portal will intercept it.
#[cfg(windows)]
fn open_login_window() {
    let _ = Command::new("explorer").arg(PORTAL_FALLBACK_URL).spawn();
}

/// Renders a globe glyph in the given colour. `alpha` scales overall opacity.
fn render_rgba(rgb: [u8; 3], alpha: f32) -> Vec<u8> {
    let s = ICON_SIZE as f32;
    let c = s / 2.0;
    let stroke = s * 0.085;
    let r = c - stroke;
    // Meridian ellipse semi-axes.
    let (ma, mb) = (r * 0.42, r);

    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let dist = (dx * dx + dy * dy).sqrt();
            let rim = (dist - r).abs();
            // Interior lines are clipped to the globe.
            let inner = if dist <= r {
                let equator = dy.abs();
                let meridian = ((dx / ma).powi(2) + (dy / mb).powi(2)).sqrt();
                let meridian = (meridian - 1.0).abs() * ma;
                equator.min(meridian)
            } else {
                f32::MAX
            };
            let d = rim.min(inner) - stroke / 2.0;
            let coverage = (0.5 - d).clamp(0.0, 1.0) * alpha;
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], (coverage * 255.0).round() as u8]);
        }
    }
    rgba
}

fn render_icon(rgb: [u8; 3], alpha: f32) -> Icon {
    Icon::from_rgba(render_rgba(rgb, alpha), ICON_SIZE, ICON_SIZE).expect("valid icon buffer")
}

fn throb_frames(rgb: [u8; 3]) -> Vec<Icon> {
    (0..THROB_FRAMES)
        .map(|i| {
            let phase = i as f32 / THROB_FRAMES as f32 * std::f32::consts::TAU;
            let alpha = 0.2 + 0.8 * (0.5 + 0.5 * phase.cos());
            render_icon(rgb, alpha)
        })
        .collect()
}

/// Rendered black and shown as a template so macOS tints it for light/dark mode.
#[cfg(target_os = "macos")]
fn online_rgb() -> [u8; 3] {
    [0, 0, 0]
}

/// Windows doesn't tint tray icons, so match the taskbar theme ourselves.
#[cfg(windows)]
fn online_rgb() -> [u8; 3] {
    if win::light_taskbar() {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    }
}

#[cfg(target_os = "macos")]
fn set_online_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon_templated(Some(icon))
}

#[cfg(windows)]
fn set_online_icon(tray: &TrayIcon, icon: Icon) -> tray_icon::Result<()> {
    tray.set_icon(Some(icon))
}

struct Icons {
    online_rgb: [u8; 3],
    online: Icon,
    captive: Vec<Icon>,
    offline: Vec<Icon>,
}

impl Icons {
    fn new() -> Self {
        let online_rgb = online_rgb();
        Self {
            online_rgb,
            online: render_icon(online_rgb, 1.0),
            captive: throb_frames([255, 159, 10]),
            offline: throb_frames([255, 59, 48]),
        }
    }

    fn frames(&self, status: Status) -> Option<&[Icon]> {
        match status {
            Status::Captive => Some(&self.captive),
            Status::Offline => Some(&self.offline),
            Status::Online | Status::Unknown => None,
        }
    }
}

struct App {
    tray: TrayIcon,
    icons: Icons,
    status_item: MenuItem,
    status: Status,
    frame: usize,
}

impl App {
    fn set_status(&mut self, status: Status) {
        if status == self.status {
            return;
        }
        let previous = self.status;
        self.status = status;
        self.frame = 0;
        eprintln!("netwatch: {:?} -> {:?}", previous, status);
        self.status_item.set_text(status.label());
        let _ = self
            .tray
            .set_tooltip(Some(format!("netwatch: {}", status.label())));
        self.redraw();

        if status == Status::Captive && previous != Status::Captive {
            open_login_window();
        }
    }

    fn redraw(&self) {
        let _ = match self.icons.frames(self.status) {
            Some(frames) => self.tray.set_icon(Some(frames[self.frame].clone())),
            None => set_online_icon(&self.tray, self.icons.online.clone()),
        };
    }

    /// Re-renders the online icon if its colour should change, e.g. after the
    /// Windows taskbar switches between light and dark.
    fn refresh_theme(&mut self) {
        let rgb = online_rgb();
        if rgb != self.icons.online_rgb {
            self.icons.online_rgb = rgb;
            self.icons.online = render_icon(rgb, 1.0);
            self.redraw();
        }
    }

    fn tick(&mut self) {
        if let Some(frames) = self.icons.frames(self.status) {
            self.frame = (self.frame + 1) % frames.len();
            self.redraw();
        }
    }
}

fn main() -> ExitCode {
    let command = std::env::args().nth(1);
    #[cfg(windows)]
    if command.is_some() {
        win::attach_console();
    }
    let result = match command.as_deref() {
        None => run(),
        Some("install") => autostart::install(),
        Some("uninstall") => autostart::uninstall(),
        Some(_) => {
            eprintln!("usage: netwatch [install | uninstall]");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("netwatch: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> std::io::Result<()> {
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |e| {
        let _ = proxy.send_event(UserEvent::Menu(e));
    }));

    let status_item = MenuItem::new(Status::Unknown.label(), false, None);
    let login_item = MenuItem::new("Open Login Window…", true, None);
    let check_item = MenuItem::new("Check Now", true, None);
    let quit_item = MenuItem::new("Quit netwatch", true, None);
    let menu = Menu::new();
    menu.append_items(&[
        &status_item,
        &PredefinedMenuItem::separator(),
        &login_item,
        &check_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])
    .expect("build menu");

    let checker = spawn_checker(event_loop.create_proxy());
    let mut app: Option<App> = None;
    let mut next_frame = Instant::now();

    event_loop.run(move |event, _, control_flow| {
        match event {
            // The tray icon must be created once the run loop is live.
            Event::NewEvents(StartCause::Init) => {
                let icons = Icons::new();
                let tray = TrayIconBuilder::new()
                    .with_menu(Box::new(menu.clone()))
                    .with_autosave_name("netwatch")
                    .with_tooltip(format!("netwatch: {}", Status::Unknown.label()))
                    .build()
                    .expect("create tray icon");
                let new_app = App {
                    tray,
                    icons,
                    status_item: status_item.clone(),
                    status: Status::Unknown,
                    frame: 0,
                };
                new_app.redraw();
                app = Some(new_app);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if let Some(app) = app.as_mut() {
                    app.tick();
                }
                next_frame = Instant::now() + FRAME_INTERVAL;
            }
            Event::UserEvent(UserEvent::Status(status)) => {
                if let Some(app) = app.as_mut() {
                    let was_throbbing = app.status.throbs();
                    app.refresh_theme();
                    app.set_status(status);
                    if status.throbs() && !was_throbbing {
                        next_frame = Instant::now() + FRAME_INTERVAL;
                    }
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == login_item.id() {
                    open_login_window();
                } else if e.id == check_item.id() {
                    let _ = checker.send(());
                } else if e.id == quit_item.id() {
                    *control_flow = ControlFlow::Exit;
                    return;
                }
            }
            _ => {}
        }

        *control_flow = match app.as_ref() {
            Some(app) if app.status.throbs() => ControlFlow::WaitUntil(next_frame),
            _ => ControlFlow::Wait,
        };
    });
}
