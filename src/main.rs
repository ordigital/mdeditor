// No console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod base64;
mod files;
mod fonts;
mod highlight;
mod i18n;
mod latex;
mod md;

use std::path::PathBuf;
use std::sync::mpsc;

fn main() -> eframe::Result {
    let files: Vec<PathBuf> =
        std::env::args_os().skip(1).map(PathBuf::from).map(|p| std::path::absolute(&p).unwrap_or(p)).collect();

    let (tx, rx) = mpsc::channel();
    let listener = match single_instance::claim(&files) {
        Some(l) => l,
        None => return Ok(()), // handed the files over to the running instance
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("MDEdit")
            .with_app_id("mdedit")
            .with_icon(icon())
            .with_inner_size([1000.0, 760.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "MDEdit",
        options,
        Box::new(move |cc| {
            single_instance::serve(listener, tx, cc.egui_ctx.clone());
            Ok(Box::new(app::App::new(cc, files, rx)))
        }),
    );
    single_instance::cleanup();
    result
}

/// Window icon (used on X11 and Windows; on Wayland the icon comes from the installed .desktop file).
fn icon() -> eframe::egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).expect("icon.png").into_rgba8();
    eframe::egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}

#[cfg(unix)]
mod single_instance {
    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;

    pub fn socket_path() -> PathBuf {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let user = std::env::var("USER").unwrap_or_default();
        dir.join(format!("mdedit-{user}.sock"))
    }

    /// Returns a listener if we are the first instance; otherwise sends `files` to it and returns None.
    pub fn claim(files: &[PathBuf]) -> Option<Option<UnixListener>> {
        let path = socket_path();
        if let Ok(mut s) = UnixStream::connect(&path) {
            let msg: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
            if s.write_all(msg.join("\n").as_bytes()).is_ok() {
                return None;
            }
        }
        let _ = std::fs::remove_file(&path);
        Some(UnixListener::bind(&path).ok())
    }

    pub fn serve(listener: Option<UnixListener>, tx: Sender<Vec<PathBuf>>, ctx: eframe::egui::Context) {
        let Some(listener) = listener else { return };
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut msg = String::new();
                let _ = stream.read_to_string(&mut msg);
                let files = msg.lines().filter(|l| !l.is_empty()).map(PathBuf::from).collect();
                if tx.send(files).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
    }

    pub fn cleanup() {
        let _ = std::fs::remove_file(socket_path());
    }
}

#[cfg(not(unix))]
mod single_instance {
    use std::path::PathBuf;
    use std::sync::mpsc::Sender;

    pub fn claim(_: &[PathBuf]) -> Option<Option<()>> {
        Some(None)
    }
    pub fn serve(_: Option<()>, _: Sender<Vec<PathBuf>>, _: eframe::egui::Context) {}
    pub fn cleanup() {}
}
