use std::{
    fs,
    path::PathBuf,
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
};

use active_win_pos_rs::get_active_window;
use regex::RegexSet;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const WINDOW_SIZE: f64 = 128.0;
const POSITION_FILE_NAME: &str = "horizontal-position";

// Active-window APIs return different identities on each platform: process
// descriptions on Windows, bundle owner names on macOS and WM_CLASS on Linux.
// Keep the patterns anchored so short names such as `code` and `idea` do not
// accidentally match unrelated applications like Decode or IdeaBoard.
static IDE_PATTERNS: LazyLock<RegexSet> = LazyLock::new(|| {
    RegexSet::new([
        // Visual Studio Code and VSCodium variants.
        r"(?ix)^(?:microsoft[\s._-]*)?(?:visual[\s._-]*studio[\s._-]*code|vs[\s._-]*code|code)(?:[\s._-]*(?:insiders|oss))?(?:\.exe)?$",
        r"(?ix)^(?:vscodium|codium)(?:[\s._-]*insiders)?(?:\.exe)?$",
        // JetBrains IDEs. `air` is intentionally qualified to avoid matching
        // unrelated applications with that very generic name.
        r"(?ix)^(?:jetbrains[\s._-]*)?(?:intellij(?:[\s._-]*idea)?|idea(?:64)?|pycharm(?:64)?|webstorm(?:64)?|phpstorm(?:64)?|rubymine(?:64)?|clion(?:64)?|goland(?:64)?|rider(?:64)?|datagrip(?:64)?|dataspell(?:64)?|rustrover(?:64)?|android[\s._-]*studio|studio64|mps|aqua|fleet|appcode)(?:[\s._-]*(?:community|professional|ultimate|educational|preview|eap|ce|ue))?(?:\.exe)?$",
        r"(?ix)^jetbrains[\s._-]*air(?:[\s._-]*(?:preview|eap))?(?:\.exe)?$",
        // Current AI-first and lightweight editors.
        r"(?ix)^(?:cursor|windsurf|kiro|trae|zed|zeditor|dev\.zed\.zed(?:-preview)?|lapce|pulsar|positron|void|antigravity)(?:[\s._-]*(?:insiders|next|preview))?(?:\.exe)?$",
        // Established desktop IDEs and code editors.
        r"(?ix)^(?:microsoft[\s._-]*)?(?:visual[\s._-]*studio|devenv)(?:[\s._-]*20\d{2})?(?:\.exe)?$",
        r"(?ix)^(?:xcode|eclipse(?:[\s._-]*ide)?|apache[\s._-]*netbeans|netbeans|qt[\s._-]*creator|qtcreator|kdevelop|gnome[\s._-]*builder|org\.gnome\.builder|geany|kate|sublime[\s._-]*text|sublime_text|nova|spyder|thonny|wing(?:[\s._-]*ide)?|arduino(?:[\s._-]*ide)?|rstudio|dbeaver|sql[\s._-]*developer|sqldeveloper|neovide|goneovim|nvim[\s._-]*qt|macvim|emacs)(?:\.exe)?$",
        // IDE-like game development environments.
        r"(?ix)^(?:unity(?:[\s._-]*editor)?|unrealeditor|unreal[\s._-]*editor|godot(?:[\s._-]*engine)?)(?:\.exe)?$",
    ])
    .expect("IDE match patterns must be valid")
});

fn is_ide(app_name: &str) -> bool {
    IDE_PATTERNS.is_match(app_name.trim())
}

#[cfg(target_os = "linux")]
fn is_gnome_wayland() -> bool {
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .is_ok_and(|session| session.eq_ignore_ascii_case("wayland"));
    let is_gnome = std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktop| {
        desktop
            .split(':')
            .any(|part| part.eq_ignore_ascii_case("gnome"))
    });

    is_wayland && is_gnome
}

#[cfg(target_os = "linux")]
fn gnome_wayland_active_app() -> Option<String> {
    use xcb::x;

    let (connection, screen_index) = xcb::Connection::connect(None).ok()?;
    let root = connection
        .get_setup()
        .roots()
        .nth(screen_index as usize)?
        .root();

    let active_window_atom = connection.send_request(&x::InternAtom {
        only_if_exists: true,
        name: b"_NET_ACTIVE_WINDOW",
    });
    let window_state_atom = connection.send_request(&x::InternAtom {
        only_if_exists: true,
        name: b"_NET_WM_STATE",
    });
    let focused_atom = connection.send_request(&x::InternAtom {
        only_if_exists: true,
        name: b"_NET_WM_STATE_FOCUSED",
    });

    let active_window_atom = connection.wait_for_reply(active_window_atom).ok()?.atom();
    let window_state_atom = connection.wait_for_reply(window_state_atom).ok()?.atom();
    let focused_atom = connection.wait_for_reply(focused_atom).ok()?.atom();

    if active_window_atom == x::ATOM_NONE
        || window_state_atom == x::ATOM_NONE
        || focused_atom == x::ATOM_NONE
    {
        return None;
    }

    let active_window = connection.send_request(&x::GetProperty {
        delete: false,
        window: root,
        property: active_window_atom,
        r#type: x::ATOM_WINDOW,
        long_offset: 0,
        long_length: 1,
    });
    let active_window = connection.wait_for_reply(active_window).ok()?;
    let active_window = active_window.value::<x::Window>().first().copied()?;

    // Mutter leaves the last X11 window in _NET_ACTIVE_WINDOW when focus moves
    // to a native Wayland window, but removes this focused state atom.
    let window_state = connection.send_request(&x::GetProperty {
        delete: false,
        window: active_window,
        property: window_state_atom,
        r#type: x::ATOM_ATOM,
        long_offset: 0,
        long_length: 64,
    });
    let window_state = connection.wait_for_reply(window_state).ok()?;
    if !window_state.value::<x::Atom>().contains(&focused_atom) {
        return None;
    }

    let window_class = connection.send_request(&x::GetProperty {
        delete: false,
        window: active_window,
        property: x::ATOM_WM_CLASS,
        r#type: x::ATOM_STRING,
        long_offset: 0,
        long_length: 1024,
    });
    let window_class = connection.wait_for_reply(window_class).ok()?;
    let app_name = String::from_utf8_lossy(window_class.value::<u8>())
        .split('\0')
        .rfind(|part| !part.is_empty())
        .unwrap_or_default()
        .to_owned();

    Some(app_name)
}

fn current_ide_mode() -> bool {
    #[cfg(target_os = "linux")]
    if is_gnome_wayland() {
        return gnome_wayland_active_app().is_some_and(|app_name| is_ide(&app_name));
    }

    get_active_window().is_ok_and(|active_window| is_ide(&active_window.app_name))
}

fn clamp_horizontal_position(
    monitor_x: i32,
    monitor_width: u32,
    window_width: u32,
    requested_x: i32,
) -> i32 {
    let available_width = monitor_width.saturating_sub(window_width);
    let max_x = monitor_x.saturating_add(i32::try_from(available_width).unwrap_or(i32::MAX));

    requested_x.clamp(monitor_x, max_x)
}

fn normalized_horizontal_position(
    monitor_x: i32,
    monitor_width: u32,
    window_width: u32,
    requested_x: i32,
) -> f64 {
    let available_width = monitor_width.saturating_sub(window_width);
    if available_width == 0 {
        return 0.0;
    }

    let x = clamp_horizontal_position(monitor_x, monitor_width, window_width, requested_x);
    let offset = i64::from(x) - i64::from(monitor_x);

    (offset as f64 / f64::from(available_width)).clamp(0.0, 1.0)
}

fn horizontal_position_from_fraction(
    monitor_x: i32,
    monitor_width: u32,
    window_width: u32,
    fraction: f64,
) -> i32 {
    let available_width = monitor_width.saturating_sub(window_width);
    let offset = (f64::from(available_width) * fraction.clamp(0.0, 1.0)).round() as i32;

    monitor_x.saturating_add(offset)
}

fn position_file_path(app: &AppHandle) -> tauri::Result<PathBuf> {
    Ok(app.path().app_data_dir()?.join(POSITION_FILE_NAME))
}

fn load_horizontal_position(window: &WebviewWindow) -> Option<f64> {
    let path = position_file_path(window.app_handle()).ok()?;
    let fraction = fs::read_to_string(path).ok()?.trim().parse::<f64>().ok()?;

    fraction.is_finite().then(|| fraction.clamp(0.0, 1.0))
}

fn persist_horizontal_position(window: &WebviewWindow, fraction: f64) -> Result<(), String> {
    let path = position_file_path(window.app_handle()).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    fs::write(path, format!("{:.8}\n", fraction.clamp(0.0, 1.0))).map_err(|error| error.to_string())
}

fn position_along_bottom(window: &WebviewWindow, requested_x: i32) -> tauri::Result<f64> {
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(1.0);
    };

    let monitor_position = monitor.position();
    let monitor_size = monitor.size();
    let window_size = window.outer_size()?;
    let x = clamp_horizontal_position(
        monitor_position.x,
        monitor_size.width,
        window_size.width,
        requested_x,
    );
    let y = monitor_position.y + monitor_size.height.saturating_sub(window_size.height) as i32;

    window.set_position(PhysicalPosition::new(x, y))?;

    Ok(normalized_horizontal_position(
        monitor_position.x,
        monitor_size.width,
        window_size.width,
        x,
    ))
}

fn position_at_bottom_right(window: &WebviewWindow) -> tauri::Result<()> {
    position_along_bottom(window, i32::MAX).map(|_| ())
}

fn position_at_saved_location(window: &WebviewWindow) -> tauri::Result<()> {
    let fraction = load_horizontal_position(window).unwrap_or(1.0);
    let Some(monitor) = window.primary_monitor()? else {
        return Ok(());
    };
    let x = horizontal_position_from_fraction(
        monitor.position().x,
        monitor.size().width,
        window.outer_size()?.width,
        fraction,
    );

    position_along_bottom(window, x).map(|_| ())
}

#[tauri::command]
fn cat_drag_origin(window: WebviewWindow) -> Result<(i32, f64), String> {
    let x = window
        .outer_position()
        .map_err(|error| error.to_string())?
        .x;
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;

    Ok((x, scale_factor))
}

#[tauri::command]
fn move_cat_along_bottom(window: WebviewWindow, x: i32, persist: bool) -> Result<(), String> {
    let fraction = position_along_bottom(&window, x).map_err(|error| error.to_string())?;
    if persist {
        persist_horizontal_position(&window, fraction)?;
    }

    Ok(())
}

fn apply_desktop_window_state(window: &WebviewWindow) -> tauri::Result<()> {
    window.set_always_on_top(true)?;
    window.set_skip_taskbar(true)?;
    window.set_focusable(false)?;

    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::GtkWindowExt;

        let native_window = window.gtk_window()?;
        native_window.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
        native_window.set_keep_above(true);
        native_window.set_skip_pager_hint(true);
        native_window.set_skip_taskbar_hint(true);
        native_window.set_accept_focus(false);
        native_window.set_focus_on_map(false);
        native_window.set_urgency_hint(false);
        native_window.stick();

        let window_for_webview = window.clone();

        // WebKitGTK reports a 200x200 natural size. With a 200% desktop scale
        // that used to force the supposedly 128x128 pet window to 400x400.
        // Lower the WebView request before locking the top-level window size.
        window.with_webview(move |webview| {
            use gtk::prelude::{GtkWindowExt, WidgetExt};

            let native_window = window_for_webview
                .gtk_window()
                .expect("the Linux window must have a GTK handle");
            webview.inner().set_size_request(1, 1);
            native_window.resize(WINDOW_SIZE as i32, WINDOW_SIZE as i32);
            native_window.set_resizable(false);
            native_window.set_keep_above(true);
            native_window.set_skip_pager_hint(true);
            native_window.set_skip_taskbar_hint(true);
            native_window.set_accept_focus(false);
            native_window.set_focus_on_map(false);
            native_window.set_urgency_hint(false);
            native_window.stick();
            native_window.show_all();
            native_window.set_urgency_hint(false);

            let window_for_position = window_for_webview.clone();
            gtk::glib::idle_add_local_once(move || {
                let _ = position_at_saved_location(&window_for_position);
            });
        })?;

        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        window.set_size(tauri::LogicalSize::new(WINDOW_SIZE, WINDOW_SIZE))?;
        window.set_resizable(false)?;

        #[cfg(target_os = "macos")]
        window.set_visible_on_all_workspaces(true)?;

        position_at_saved_location(window)?;
        window.show()?;

        Ok(())
    }
}

fn setup_tray(app: &mut App) -> tauri::Result<()> {
    let toggle_visibility =
        MenuItem::with_id(app, "toggle-visibility", "Hide cat", true, None::<&str>)?;
    let reset_position = MenuItem::with_id(
        app,
        "reset-position",
        "Move to bottom-right",
        true,
        None::<&str>,
    )?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Start with system",
        true,
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let settings = Submenu::with_items(app, "Settings", true, &[&autostart])?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle_visibility,
            &reset_position,
            &settings,
            &separator,
            &quit,
        ],
    )?;

    let toggle_visibility_for_events = toggle_visibility.clone();
    let autostart_for_events = autostart.clone();
    let mut tray = TrayIconBuilder::with_id("boba-cat")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("boba-cat")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "toggle-visibility" => {
                let Some(window) = app.get_webview_window("main") else {
                    return;
                };
                let is_visible = window.is_visible().unwrap_or(false);
                let changed = if is_visible {
                    window.hide()
                } else {
                    apply_desktop_window_state(&window)
                };

                if changed.is_ok() {
                    let text = if is_visible { "Show cat" } else { "Hide cat" };
                    let _ = toggle_visibility_for_events.set_text(text);
                }
            }
            "reset-position" => {
                if let Some(window) = app.get_webview_window("main") {
                    if position_at_bottom_right(&window).is_ok() {
                        let _ = persist_horizontal_position(&window, 1.0);
                    }
                }
            }
            "autostart" => {
                let manager = app.autolaunch();
                let was_enabled = manager.is_enabled().unwrap_or(false);
                let result = if was_enabled {
                    manager.disable()
                } else {
                    manager.enable()
                };
                let is_enabled = result.map(|_| !was_enabled).unwrap_or(was_enabled);
                let _ = autostart_for_events.set_checked(is_enabled);
            }
            "quit" => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;

    Ok(())
}

fn watch_active_window(window: WebviewWindow) {
    thread::spawn(move || {
        let mut last_ide = None;
        let mut last_emit = Instant::now();

        loop {
            let ide = current_ide_mode();
            let needs_resync = last_emit.elapsed() >= Duration::from_secs(2);

            if last_ide != Some(ide) || needs_resync {
                last_ide = Some(ide);
                last_emit = Instant::now();
                if window.emit("ide-mode", ide).is_err() {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(500));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Wayland intentionally prevents applications from choosing an absolute
    // position and, depending on the compositor, from staying above other
    // windows. XWayland preserves the desktop-widget behaviour on Linux.
    #[cfg(target_os = "linux")]
    std::env::set_var("GDK_BACKEND", "x11");

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            cat_drag_origin,
            move_cat_along_bottom
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                window.app_handle().exit(0);
            }
        })
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("the main window must be configured");

            apply_desktop_window_state(&window)?;
            setup_tray(app)?;
            watch_active_window(window);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running boba-cat");
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_horizontal_position, horizontal_position_from_fraction, is_ide,
        normalized_horizontal_position,
    };

    #[test]
    fn identifies_current_ide_app_identities_case_insensitively() {
        for app_name in [
            "code",
            "Code - Insiders",
            "Code - OSS",
            "Visual Studio Code",
            "VSCodium.exe",
            "jetbrains-idea-ce",
            "PyCharm",
            "Android Studio",
            "Cursor",
            "Windsurf-next",
            "dev.zed.Zed",
            "Positron",
            "devenv.exe",
            "Microsoft Visual Studio 2022",
            "org.gnome.Builder",
            "QtCreator",
            "DBeaver",
            "Godot Engine",
        ] {
            assert!(is_ide(app_name), "expected {app_name:?} to be an IDE");
        }
    }

    #[test]
    fn rejects_apps_that_only_contain_a_short_ide_name() {
        for app_name in [
            "Decode",
            "Barcode Studio",
            "IdeaBoard",
            "Rider Logistics",
            "Air",
            "Firefox",
            "ChatGPT",
        ] {
            assert!(!is_ide(app_name), "expected {app_name:?} not to be an IDE");
        }
    }

    #[test]
    fn clamps_dragging_to_the_monitor_width() {
        assert_eq!(clamp_horizontal_position(0, 2880, 256, -100), 0);
        assert_eq!(clamp_horizontal_position(0, 2880, 256, 1200), 1200);
        assert_eq!(clamp_horizontal_position(0, 2880, 256, 3000), 2624);
        assert_eq!(clamp_horizontal_position(-1920, 1920, 256, 0), -256);
    }

    #[test]
    fn stores_and_restores_position_as_a_resolution_independent_fraction() {
        assert_eq!(normalized_horizontal_position(0, 2880, 256, 1312), 0.5);
        assert_eq!(horizontal_position_from_fraction(0, 1920, 128, 0.5), 896);
        assert_eq!(
            horizontal_position_from_fraction(-1920, 1920, 256, 1.0),
            -256
        );
    }
}
