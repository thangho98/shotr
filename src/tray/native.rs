//! System tray on Windows and macOS, via `tray-icon`.
//!
//! Unlike the SNI path, the icon cannot simply be held: `tray-icon` needs a
//! platform event loop on the thread that built it, and on macOS that thread
//! must be the main one. winit is already in the tree through eframe, so the
//! daemon borrows its loop rather than standing up a second one.

use std::collections::HashMap;

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

use super::{Command, POLL};
use crate::i18n::t;
use crate::render::icon::icon_image;

/// tray-icon's macOS backend pins the image to 18pt tall and scales the width
/// to match, so 36px is exactly 2× for a Retina bar. Windows draws the
/// notification area at 16px logical, which is 32px at 200% scaling.
#[cfg(target_os = "macos")]
const ICON_PX: u32 = 36;
#[cfg(not(target_os = "macos"))]
const ICON_PX: u32 = 32;

/// Own the thread until the user quits, and return the process exit code.
///
/// `tick` sees each command the menu produced, and `None` on every idle pass —
/// which is where the daemon checks for requests from later launches.
/// Returning `false` stops the loop.
pub fn run(tick: impl FnMut(Option<Command>) -> bool) -> i32 {
    let mut builder = EventLoop::builder();

    // An app with no window still claims a Dock icon and a menu bar unless it
    // says otherwise. Doing this at runtime rather than with `LSUIElement` in
    // Info.plist is deliberate: that key applies to the whole bundle, so it
    // would strip the editor of its Dock icon and its menu as well.
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }

    let event_loop = match builder.build() {
        Ok(loop_) => loop_,
        Err(e) => {
            eprintln!("Could not start the event loop the tray needs: {e}");
            return 1;
        }
    };
    event_loop.set_control_flow(ControlFlow::wait_duration(POLL));

    let mut daemon = Daemon {
        tick,
        tray: None,
        actions: HashMap::new(),
        code: 0,
    };
    if let Err(e) = event_loop.run_app(&mut daemon) {
        eprintln!("The tray event loop stopped: {e}");
        return 1;
    }
    daemon.code
}

struct Daemon<F> {
    tick: F,
    /// Dropping this takes the icon out of the bar, so it has to outlive the
    /// call that built it.
    tray: Option<TrayIcon>,
    actions: HashMap<MenuId, Command>,
    code: i32,
}

impl<F> Daemon<F> {
    /// Swap in a menu built from what is on screen now.
    ///
    /// The SNI path is asked for its menu each time one opens, so it never
    /// needs this. Here the menu is an object the system owns and shows without
    /// telling us, so a list of windows built at startup would name windows
    /// that closed hours ago. Rebuilding replaces every id, hence `actions` in
    /// the same breath — the two must never disagree.
    fn refresh_menu(&mut self) {
        let Some(tray) = self.tray.as_ref() else {
            return;
        };
        match menu() {
            Ok((menu, actions)) => {
                tray.set_menu(Some(Box::new(menu)));
                self.actions = actions;
            }
            // Keep the menu that already works rather than leaving none.
            Err(e) => eprintln!("Could not rebuild the tray menu: {e}"),
        }
    }
}

impl<F: FnMut(Option<Command>) -> bool> ApplicationHandler for Daemon<F> {
    /// macOS refuses a status item before NSApplication is running, and this is
    /// the first callback that happens after it is — which is why the icon is
    /// built here and not before the loop starts.
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }
        match build() {
            Ok((tray, actions)) => {
                self.tray = Some(tray);
                self.actions = actions;
                eprintln!("shotr is running in the system tray. Click the icon to capture.");
            }
            Err(e) => {
                eprintln!("Could not create the tray icon: {e}");
                self.code = 1;
                event_loop.exit();
            }
        }
    }

    /// The daemon owns no window, so nothing can send one an event.
    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(command) = self.actions.get(&event.id).cloned()
                && !(self.tick)(Some(command))
            {
                event_loop.exit();
                return;
            }
        }

        // A click carries no action of its own — it opens the menu — but the
        // window list behind that menu goes stale in seconds, so something has
        // to say when to rebuild it.
        //
        // Not the click. `mouseDown:` reports the click and *then* opens the
        // menu, in that order and in one call, so by the time we see the event
        // the menu is already on screen; swapping it there replaces what the
        // system is showing, and macOS answers by closing it. The menu appeared
        // to vanish the instant it was clicked.
        //
        // `Enter` is the one that works: it arrives while the pointer is still
        // travelling towards the icon, with no menu up. It is skipped when a
        // click landed in the same batch, which is the pointer-slam case — that
        // menu is already open too. `Move` is ignored outright, because it
        // repeats for as long as the pointer rests on the icon.
        let mut approaching = false;
        let mut opened = false;
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::Enter { .. } => approaching = true,
                TrayIconEvent::Click { .. } | TrayIconEvent::DoubleClick { .. } => opened = true,
                _ => {}
            }
        }
        if approaching && !opened {
            self.refresh_menu();
        }

        if !(self.tick)(None) {
            event_loop.exit();
            return;
        }
        event_loop.set_control_flow(ControlFlow::wait_duration(POLL));
    }
}

/// Build the icon and the menu behind it.
fn build() -> Result<(TrayIcon, HashMap<MenuId, Command>), Box<dyn std::error::Error>> {
    let (menu, actions) = menu()?;

    let image = icon_image(ICON_PX);
    let icon = Icon::from_rgba(image.into_raw(), ICON_PX, ICON_PX)?;

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("shotr")
        .with_icon(icon)
        .with_menu_on_left_click(true)
        .build()?;
    Ok((tray, actions))
}

/// The three ways to choose what gets captured, and nothing else. The editor
/// deliberately offers no way to change its mind afterwards, so this menu is
/// the whole decision.
///
/// Every clickable entry is mapped to the command it stands for, because the
/// ids are minted here and mean nothing to anyone who did not build this menu.
fn menu() -> Result<(Menu, HashMap<MenuId, Command>), Box<dyn std::error::Error>> {
    let mut actions = HashMap::new();
    let menu = Menu::new();

    let region = MenuItem::new(t("Capture a region…"), true, None);
    actions.insert(region.id().clone(), Command::CaptureRegion);
    let open = MenuItem::new(t("Open image…"), true, None);
    actions.insert(open.id().clone(), Command::OpenFile);
    let quit = MenuItem::new(t("Quit"), true, None);
    actions.insert(quit.id().clone(), Command::Quit);

    let screens = Submenu::new(t("Capture a whole screen"), true);
    let everything = MenuItem::new(t("All screens together"), true, None);
    actions.insert(everything.id().clone(), Command::CaptureFull);
    screens.append(&everything)?;
    let monitors = crate::capture::list_monitors();
    if !monitors.is_empty() {
        screens.append(&PredefinedMenuItem::separator())?;
        for screen in monitors {
            let item = MenuItem::new(format!("{} — {}", screen.index + 1, screen.name), true, None);
            actions.insert(item.id().clone(), Command::CaptureMonitor(screen.index));
            screens.append(&item)?;
        }
    }

    let windows = Submenu::new(t("Capture a window"), true);
    let listed = crate::winlist::list();
    if listed.is_empty() {
        windows.append(&MenuItem::new(t("(no windows could be read)"), false, None))?;
    } else {
        for window in listed {
            let item = MenuItem::new(window.label(), true, None);
            actions.insert(item.id().clone(), Command::CaptureWindow(window.identifier));
            windows.append(&item)?;
        }
    }

    menu.append_items(&[
        &region,
        &screens,
        &windows,
        &PredefinedMenuItem::separator(),
        &open,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    Ok((menu, actions))
}
