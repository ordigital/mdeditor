//! Simple file browser panel: ".." first, then folders, then files, alphabetically.

use std::io::Read;
use std::path::{Path, PathBuf};

use eframe::egui::{self, RichText, Ui};

use crate::i18n::{Lang, tr};
use crate::md;

/// Files larger than this are not opened.
pub const MAX_SIZE: u64 = 5 * 1024 * 1024;

pub enum Event {
    Open(PathBuf),
    Close,
    /// "Create new" was clicked.
    New,
    Delete(PathBuf),
}

pub struct Browser {
    pub dir: PathBuf,
    entries: Vec<(String, bool)>,
    loaded_at: f64,
}

impl Browser {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, entries: vec![], loaded_at: f64::NEG_INFINITY }
    }

    fn refresh(&mut self, now: f64) {
        if now - self.loaded_at < 2.0 {
            return;
        }
        self.loaded_at = now;
        let mut entries: Vec<(String, bool)> = std::fs::read_dir(&self.dir)
            .map(|rd| rd.flatten().map(|e| (e.file_name().to_string_lossy().into_owned(), e.path().is_dir())).collect())
            .unwrap_or_default();
        entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        self.entries = entries;
    }

    /// Re-reads the folder on the next frame (after creating or deleting files).
    pub fn reload(&mut self) {
        self.loaded_at = f64::NEG_INFINITY;
    }

    fn enter(&mut self, dir: PathBuf) {
        self.dir = dir;
        self.loaded_at = f64::NEG_INFINITY;
    }

    /// Shows the panel: a header (folder name opens a folder picker, × closes the panel)
    /// and the listing, where a click enters a folder or opens a file.
    pub fn ui(&mut self, ui: &mut Ui, open: &[&Path], active: Option<&Path>, lang: Lang) -> Option<Event> {
        self.refresh(ui.input(|i| i.time));
        let name =
            self.dir.file_name().map_or_else(|| self.dir.display().to_string(), |n| n.to_string_lossy().into_owned());
        let mut event = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = egui::Button::new(RichText::new("×").size(15.0).color(md::MUTED)).frame(false);
                if ui.add(close).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    event = Some(Event::Close);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let label = egui::Label::new(RichText::new(name).strong().color(md::TEXT))
                        .sense(egui::Sense::click())
                        .selectable(false)
                        .truncate();
                    let r = ui.add(label).on_hover_cursor(egui::CursorIcon::PointingHand);
                    if r.on_hover_text(self.dir.display().to_string()).clicked()
                        && let Some(dir) = rfd::FileDialog::new().set_directory(&self.dir).pick_folder()
                    {
                        self.enter(dir);
                    }
                });
            });
        });
        ui.add_space(2.0);
        ui.separator();

        let mut picked = None;
        let mut go = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                if row(ui, &format!("+  {}", tr(lang, "Create new")), md::LINK, false).clicked() {
                    event = Some(Event::New);
                }
                if let Some(parent) = self.dir.parent()
                    && row(ui, "..", md::MUTED, false).clicked()
                {
                    go = Some(parent.to_path_buf());
                }
                for (name, is_dir) in &self.entries {
                    let path = self.dir.join(name);
                    let r = if *is_dir {
                        row(ui, &format!("{name}/"), md::MUTED, false)
                    } else {
                        let color = if open.contains(&path.as_path()) { md::LINK } else { md::TEXT };
                        let r = row(ui, name, color, active == Some(path.as_path()));
                        // Trash icon at the right edge of the hovered file.
                        if ui.rect_contains_pointer(r.rect) {
                            let icon =
                                egui::Rect::from_min_max(egui::pos2(r.rect.max.x - 26.0, r.rect.min.y), r.rect.max);
                            let del = egui::Button::new(RichText::new("🗑").color(md::MUTED)).frame(false);
                            let del = ui.put(icon, del).on_hover_text(tr(lang, "Delete…"));
                            if del.clicked() {
                                event = Some(Event::Delete(path.clone()));
                            }
                        }
                        r
                    };
                    if r.clicked() {
                        if *is_dir {
                            go = Some(path);
                        } else {
                            picked = Some(path);
                        }
                    }
                }
            });
        });
        if let Some(d) = go {
            self.enter(d);
        }
        event.or(picked.map(Event::Open))
    }
}

fn row(ui: &mut Ui, text: &str, color: egui::Color32, selected: bool) -> egui::Response {
    ui.add(egui::Button::selectable(selected, RichText::new(text).color(color)).truncate())
}

/// Why a file is refused: too large, or not UTF-8 text.
pub fn open_warning(path: &Path) -> Option<&'static str> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_SIZE {
        return Some("The file is too large (over 5 MB)");
    }
    let mut buf = vec![0; 8192];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).ok()?;
    let buf = &buf[..n];
    let text = match std::str::from_utf8(buf) {
        Ok(_) => true,
        // A multi-byte character may be cut at the end of the sample.
        Err(e) => e.error_len().is_none() && n - e.valid_up_to() < 4,
    };
    (!text || buf.contains(&0)).then_some("The file is not a text file")
}
