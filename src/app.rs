use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, mpsc::Receiver};

use eframe::egui::{
    self, Align, Align2, Button, CentralPanel, Color32, Context, CornerRadius, FontFamily, FontId, Galley, Id, Key,
    KeyboardShortcut, Label, Margin, Modifiers, Panel, Pos2, Rect, RichText, ScrollArea, Sense, Shape, Stroke,
    TextBuffer, TextEdit, TextureHandle, Ui, Vec2, ViewportCommand, pos2,
    text::{CCursor, CCursorRange},
    text_edit::TextEditState,
    vec2,
};

use crate::base64;
use crate::files;
use crate::fonts;
use crate::highlight::{self, Syntax};
use crate::i18n::{Lang, tr};
use crate::markdown::{self, Fence, ListMarker};
use crate::md::{self, Deco};
use eframe::egui::epaint::FontColorTransferFunction;

const MAX_LINE_W: f32 = 720.0;
const VIEW_WIDTHS: [&str; 4] = ["Narrow", "Medium", "Wide", "Full"];

/// Interpolate between the original reading column and the original full-width layout.
fn view_geometry(full: f32, gutter: f32, width: u8) -> (f32, f32) {
    let available = (full - gutter - 48.0).max(100.0);
    let narrow = available.min(MAX_LINE_W);
    let t = f32::from(width) / 3.0;
    let column = narrow + (available - narrow) * t;
    let left = ((full - narrow) / 2.0).max(gutter + 16.0);
    (column, left + (gutter + 24.0 - left) * t)
}

const MAX_IMG_W: u32 = 1200;
/// Undo steps kept per document and entries in File → Recent.
const HISTORY: usize = 10;
const RECENT: usize = 10;
const TITLE_SCALE: f32 = 1.618;
/// Obsidian's `--background-secondary` (menu, tabs, dialogs) and a lighter status bar.
const CHROME: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
const COFFEE_LABEL: &str = "♡ Buy me a coffee";
const COFFEE_URL: &str = "https://buymeacoffee.com/adamstrad";
const STATUS: Color32 = Color32::from_rgb(0x2e, 0x2e, 0x2e);

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Bold,
    Italic,
    Strike,
    Code,
    Mark,
    Link,
    Heading(u8),
    Bullet,
    Ordered,
    Task,
    Quote,
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    New,
    Open,
    Save,
    SaveAs,
    SaveAll,
    ExportPdf,
    Emoji,
    Print,
    Close,
    CloseAll,
    Copy,
    Paste,
    Find,
    Insert,
    Preferences,
    About,
    Undo,
    Redo,
    Files,
    CycleWidth,
    PastePrimary,
    Exit,
    NewPage,
    OpenRecent(usize),
    Fmt(Fmt),
}

/// Character endpoints identify a selection independently of scroll/font geometry.
#[derive(Clone, Copy)]
struct EditorSelection {
    doc_id: u64,
    range: (usize, usize),
    anchor: Pos2,
}

impl EditorSelection {
    fn key(self) -> (u64, (usize, usize)) {
        (self.doc_id, self.range)
    }
}

struct SelectionTools {
    selection: EditorSelection,
    emoji: bool,
    query: String,
    focus_search: bool,
    rect: Rect,
}

const TOOL_COLUMNS: usize = 4;
const TOOL_GAP: f32 = 3.0;
const TOOL_BUTTON: Vec2 = Vec2::splat(26.0);
const EMOJI_BUTTON: Vec2 = Vec2::splat(36.0);
const EMOJI_SEARCH_HEIGHT: f32 = 28.0;
const EMOJI_SEARCH_ID: &str = "emoji_search";

fn emoji_content_size() -> Vec2 {
    vec2(
        TOOL_COLUMNS as f32 * (EMOJI_BUTTON.x + TOOL_GAP) - TOOL_GAP,
        6.0 * (EMOJI_BUTTON.y + TOOL_GAP) - TOOL_GAP + EMOJI_SEARCH_HEIGHT + TOOL_GAP,
    )
}

fn tool_content_size() -> Vec2 {
    let rows = formatting_buttons().len().div_ceil(TOOL_COLUMNS);
    vec2(
        TOOL_COLUMNS as f32 * (TOOL_BUTTON.x + TOOL_GAP) - TOOL_GAP,
        rows as f32 * (TOOL_BUTTON.y + TOOL_GAP) - TOOL_GAP,
    )
}

struct NewPage {
    name: String,
    doc_id: u64,
    selection: (usize, usize),
    original: String,
    dir: PathBuf,
    focus: bool,
    error: Option<String>,
}

/// All formatting buttons share the existing editor actions and translated labels.
fn formatting_buttons() -> &'static [(&'static str, &'static str, Action)] {
    use Fmt::*;
    &[
        ("P", "Normal text", Action::Fmt(Heading(0))),
        ("B", "Bold", Action::Fmt(Bold)),
        ("I", "Italic", Action::Fmt(Italic)),
        ("S", "Strikethrough", Action::Fmt(Strike)),
        ("<>", "Inline code", Action::Fmt(Code)),
        ("", "Highlight", Action::Fmt(Mark)),
        ("", "Link", Action::Fmt(Link)),
        ("", "New linked page", Action::NewPage),
        ("H1", "Heading 1–6", Action::Fmt(Heading(1))),
        ("H2", "Heading 1–6", Action::Fmt(Heading(2))),
        ("H3", "Heading 1–6", Action::Fmt(Heading(3))),
        ("H4", "Heading 1–6", Action::Fmt(Heading(4))),
        ("H5", "Heading 1–6", Action::Fmt(Heading(5))),
        ("H6", "Heading 1–6", Action::Fmt(Heading(6))),
        ("", "Bullet list", Action::Fmt(Bullet)),
        ("", "Numbered list", Action::Fmt(Ordered)),
        ("", "Task (checkbox)", Action::Fmt(Task)),
        ("", "Quote", Action::Fmt(Quote)),
        ("", "Insert image…", Action::Insert),
    ]
}

/// Allocate a strict square: text/font ascent must not resize or offset grid cells.
fn tool_button(ui: &mut Ui, size: Vec2) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect(rect, visuals.corner_radius, visuals.weak_bg_fill, visuals.bg_stroke, egui::StrokeKind::Inside);
    response
}

/// Draw symbols as geometry: they must not depend on glyph coverage in the UI font.
fn formatting_button(ui: &mut Ui, icon: &str, action: Action) -> egui::Response {
    let vector = matches!(
        action,
        Action::Insert
            | Action::NewPage
            | Action::Fmt(Fmt::Mark | Fmt::Link | Fmt::Bullet | Fmt::Ordered | Fmt::Task | Fmt::Quote)
    );
    let text = RichText::new(if vector { "" } else { icon }).size(12.0);
    let text = match action {
        Action::Fmt(Fmt::Bold) => text.strong(),
        Action::Fmt(Fmt::Italic) => text.italics(),
        Action::Fmt(Fmt::Strike) => text.strikethrough(),
        _ => text,
    };
    let response = tool_button(ui, TOOL_BUTTON);
    if !vector {
        let galley = egui::WidgetText::from(text).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            TOOL_BUTTON.x - 4.0,
            egui::TextStyle::Button,
        );
        ui.painter().galley(
            response.rect.center() - galley.size() * 0.5,
            galley,
            ui.style().interact(&response).fg_stroke.color,
        );
    }
    if vector && ui.is_rect_visible(response.rect) {
        let painter = ui.painter();
        let center = response.rect.center();
        let stroke = ui.style().interact(&response).fg_stroke;
        let color = stroke.color;
        let stroke = Stroke::new(1.5, color);
        let at = |x, y| center + vec2(x, y) * 0.85;
        let line = |a, b| {
            painter.line_segment([a, b], stroke);
        };
        let rect = |a, b| {
            painter.rect_stroke(Rect::from_min_max(a, b), 1.0, stroke, egui::StrokeKind::Inside);
        };
        match action {
            Action::Fmt(Fmt::Mark) => {
                painter.add(egui::Shape::closed_line(
                    vec![at(-4.0, 1.0), at(3.0, -6.0), at(7.0, -2.0), at(0.0, 5.0)],
                    stroke,
                ));
                painter.add(egui::Shape::convex_polygon(
                    vec![at(-4.0, 1.0), at(0.0, 5.0), at(-3.0, 6.0), at(-5.0, 4.0)],
                    color,
                    Stroke::NONE,
                ));
                line(at(-8.0, 9.0), at(2.0, 9.0));
            }
            Action::Fmt(Fmt::Bullet | Fmt::Ordered) => {
                for (i, y) in [-5.0, 0.0, 5.0].into_iter().enumerate() {
                    if action == Action::Fmt(Fmt::Ordered) {
                        painter.text(
                            at(-7.0, y),
                            Align2::CENTER_CENTER,
                            (i + 1).to_string(),
                            FontId::proportional(7.0),
                            color,
                        );
                    } else {
                        painter.circle_filled(at(-7.0, y), 1.2, color);
                    }
                    line(at(-2.0, y), at(8.0, y));
                }
            }
            Action::Fmt(Fmt::Task) => {
                rect(at(-7.0, -7.0), at(7.0, 7.0));
                line(at(-4.0, 0.0), at(-1.0, 3.0));
                line(at(-1.0, 3.0), at(4.0, -3.0));
            }
            Action::Fmt(Fmt::Quote) => {
                line(at(-7.0, -7.0), at(-7.0, 7.0));
                for y in [-4.0, 0.0, 4.0] {
                    line(at(-2.0, y), at(7.0, y));
                }
            }
            Action::Insert => {
                rect(at(-9.0, -7.0), at(9.0, 7.0));
                painter.circle_filled(at(4.0, -3.0), 1.5, color);
                line(at(-7.0, 5.0), at(-2.0, -1.0));
                line(at(-2.0, -1.0), at(3.0, 5.0));
                line(at(3.0, 5.0), at(6.0, 2.0));
            }
            _ => {
                // Two diagonal capsule outlines form the familiar chain-link symbol.
                for offset in [-4.0, 4.0] {
                    let mut points = Vec::with_capacity(18);
                    for end in [-1.0_f32, 1.0] {
                        for step in 0..=8 {
                            let angle = std::f32::consts::PI * (step as f32 / 8.0 + if end < 0.0 { 0.5 } else { -0.5 });
                            let u = offset + end * 2.5 + angle.cos() * 3.0;
                            let v = angle.sin() * 3.0;
                            let diagonal = std::f32::consts::FRAC_1_SQRT_2;
                            points.push(at((u + v) * diagonal, (v - u) * diagonal));
                        }
                    }
                    painter.add(egui::Shape::closed_line(points, stroke));
                }
                if action == Action::NewPage {
                    line(at(4.0, 5.0), at(10.0, 5.0));
                    line(at(7.0, 2.0), at(7.0, 8.0));
                }
            }
        }
    }
    response
}

struct Cache {
    key: u64,
    line: Option<usize>,
    galley: Arc<Galley>,
    decos: Vec<Deco>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SyntaxMode {
    Automatic,
    Markdown,
    Language(Syntax),
}

fn disk_hash(text: &str) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hash);
    hash.finish()
}

struct Doc {
    id: u64,
    path: Option<PathBuf>,
    text: String,
    saved: String,
    disk_hash: u64,
    cache: Option<Cache>,
    analysis: highlight::Analysis,
    code_layout: highlight::LayoutCache,
    /// Text as of the last recorded change, and the undo/redo snapshots.
    prev: String,
    undo: Vec<String>,
    redo: Vec<String>,
    last_edit: f64,
    /// Effective syntax, refreshed only when the filename or tab-local mode changes.
    syntax: Option<Syntax>,
    syntax_mode: SyntaxMode,
    /// The file used Windows line endings; they are restored when saving (the text is kept as `\n`).
    crlf: bool,
    /// (characters, words), recomputed only after the text changed.
    stats: std::cell::Cell<Option<(usize, usize)>>,
}

impl Doc {
    fn new(id: u64, path: Option<PathBuf>, text: String) -> Self {
        Self {
            id,
            syntax: path.as_deref().and_then(Syntax::from_path),
            syntax_mode: SyntaxMode::Automatic,
            path,
            disk_hash: disk_hash(&text),
            saved: text.clone(),
            prev: text.clone(),
            text,
            cache: None,
            analysis: highlight::Analysis::default(),
            code_layout: highlight::LayoutCache::default(),
            undo: vec![],
            redo: vec![],
            last_edit: f64::NEG_INFINITY,
            crlf: false,
            stats: Default::default(),
        }
    }

    /// Selecting a view never edits the buffer, filename or undo history.
    fn set_syntax(&mut self, mode: SyntaxMode) {
        self.syntax_mode = mode;
        self.syntax = match mode {
            SyntaxMode::Automatic => self.path.as_deref().and_then(Syntax::from_path),
            SyntaxMode::Markdown => None,
            SyntaxMode::Language(syntax) => Some(syntax),
        };
    }

    /// (characters, words) of the text.
    fn stats(&self) -> (usize, usize) {
        self.stats.get().unwrap_or_else(|| {
            let words = self.text.split_whitespace().filter(|w| w.chars().any(char::is_alphanumeric)).count();
            let stats = (self.text.chars().count(), words);
            self.stats.set(Some(stats));
            stats
        })
    }

    /// Records a change (returns whether there was one); typing within a second of the previous
    /// change joins the same undo step.
    fn record(&mut self, now: f64) -> bool {
        if self.text == self.prev {
            return false;
        }
        self.stats.set(None);
        if now - self.last_edit > 1.0 {
            self.undo.push(std::mem::take(&mut self.prev));
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.prev = self.text.clone();
        self.last_edit = now;
        true
    }

    /// Closes the current undo step, so the next change starts a new one.
    fn checkpoint(&mut self, now: f64) {
        self.record(now);
        self.last_edit = f64::NEG_INFINITY;
    }

    /// Undo (or redo with `redo`); returns the char index where the texts start to differ.
    fn step(&mut self, now: f64, redo: bool) -> Option<usize> {
        self.record(now);
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let text = from.pop()?;
        to.push(std::mem::replace(&mut self.text, text));
        self.stats.set(None);
        self.prev = self.text.clone();
        self.last_edit = f64::NEG_INFINITY;
        let old = to.last().unwrap();
        Some(self.text.chars().zip(old.chars()).take_while(|(a, b)| a == b).count())
    }
    fn edit_id(&self) -> Id {
        Id::new(("doc", self.id))
    }
    fn dirty(&self) -> bool {
        self.text != self.saved
    }
    fn title(&self, lang: Lang) -> String {
        match self.path.as_ref().and_then(|p| p.file_name()) {
            Some(n) => n.to_string_lossy().into_owned(),
            None => tr(lang, "Untitled").to_owned(),
        }
    }
    fn stem(&self, lang: Lang) -> String {
        match self.path.as_ref().and_then(|p| p.file_stem()) {
            Some(n) => n.to_string_lossy().into_owned(),
            None => tr(lang, "Untitled").to_owned(),
        }
    }
    fn dir(&self) -> Option<PathBuf> {
        self.path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf)
    }
}

#[derive(Clone, PartialEq)]
struct Config {
    lang: Lang,
    view_width: u8,
    line_numbers: bool,
    inline_title: bool,
    files: bool,
    /// Pasted images go into the document as base64 JPEG (else into a WebP file next to it).
    embed_images: bool,
    font: String,
    weight: u16,
    bold_weight: u16,
    code_weight: u16,
    gamma: f32,
    size: f32,
    line_height: f32,
}

impl Config {
    fn look(&self) -> md::Look {
        md::Look {
            size: self.size,
            line_height: self.line_height,
            weight: self.weight,
            bold_weight: self.bold_weight,
            code_weight: self.code_weight,
        }
    }

    /// Default text settings (also restored by the Reset button in Preferences).
    fn default_text() -> Self {
        Config {
            lang: Lang::English,
            view_width: 0,
            line_numbers: false,
            inline_title: true,
            files: false,
            embed_images: true,
            font: String::new(),
            weight: 300,
            bold_weight: 600,
            code_weight: 400,
            gamma: 0.8,
            size: 17.0,
            line_height: 1.65,
        }
    }

    fn reset_text(&mut self) {
        let d = Self::default_text();
        (self.font, self.weight, self.bold_weight, self.code_weight) = (d.font, d.weight, d.bold_weight, d.code_weight);
        (self.gamma, self.size, self.line_height) = (d.gamma, d.size, d.line_height);
    }

    fn path() -> Option<PathBuf> {
        if cfg!(test) {
            return None;
        }
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("mdedit").join("config"))
    }

    fn load() -> Self {
        let defaults = Config { lang: Lang::from_env(), ..Self::default_text() };
        let text = Self::path().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
        defaults.parse(&text)
    }

    /// Applies `key=value` lines to `self`; unknown keys are ignored and numbers outside their range
    /// (or not numbers) keep the current value, so a hand-edited file cannot break the layout.
    fn parse(mut self, text: &str) -> Self {
        fn number<T: std::str::FromStr + PartialOrd>(v: &str, range: std::ops::RangeInclusive<T>, current: T) -> T {
            v.parse().ok().filter(|n| range.contains(n)).unwrap_or(current)
        }
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (v, on) = (v.trim(), v.trim() == "1");
            match k.trim() {
                "lang" => self.lang = Lang::from_code(v).unwrap_or(self.lang),
                // Settings files from before the language menu.
                "pl" if on => self.lang = Lang::Polish,
                "optimize" if !text.lines().any(|l| l.trim_start().starts_with("view_width=")) => {
                    self.view_width = if on { 0 } else { 3 }
                }
                "view_width" => self.view_width = number(v, 0..=3, self.view_width),
                "line_numbers" => self.line_numbers = on,
                "inline_title" => self.inline_title = on,
                "files" => self.files = on,
                "embed_images" => self.embed_images = on,
                "font" => self.font = v.to_owned(),
                "size" => self.size = number(v, 8.0..=48.0, self.size),
                "weight" => self.weight = number(v, 100..=900, self.weight),
                "bold_weight" => self.bold_weight = number(v, 100..=900, self.bold_weight),
                "code_weight" => self.code_weight = number(v, 200..=900, self.code_weight),
                "gamma" => self.gamma = number(v, 0.4..=1.4, self.gamma),
                "line_height" => self.line_height = number(v, 1.0..=2.5, self.line_height),
                _ => {}
            }
        }
        self
    }

    fn to_text(&self) -> String {
        let b = |v: bool| u8::from(v);
        format!(
            "lang={}\nview_width={}\nline_numbers={}\ninline_title={}\nfiles={}\nembed_images={}\nfont={}\nweight={}\nbold_weight={}\ncode_weight={}\ngamma={}\nsize={}\nline_height={}\n",
            self.lang.code(),
            self.view_width,
            b(self.line_numbers),
            b(self.inline_title),
            b(self.files),
            b(self.embed_images),
            self.font,
            self.weight,
            self.bold_weight,
            self.code_weight,
            self.gamma,
            self.size,
            self.line_height
        )
    }

    fn load_recent() -> Vec<PathBuf> {
        let text =
            Self::path().and_then(|p| std::fs::read_to_string(p.with_file_name("recent")).ok()).unwrap_or_default();
        text.lines().filter(|l| !l.is_empty()).map(PathBuf::from).take(RECENT).collect()
    }

    fn save_recent(recent: &[PathBuf]) {
        let Some(p) = Self::path() else { return };
        let text: Vec<String> = recent.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let _ = p.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(p.with_file_name("recent"), text.join("\n") + "\n");
    }

    fn save(&self) {
        let Some(p) = Self::path() else { return };
        let _ = p.parent().map(std::fs::create_dir_all);
        let _ = std::fs::write(p, self.to_text());
    }
}

#[derive(Default)]
struct Find {
    query: String,
    replace: String,
    focus: bool,
    hl: Option<(usize, usize)>,
    none: bool,
}

pub struct App {
    docs: Vec<Doc>,
    active: usize,
    next_id: u64,
    cfg: Config,
    saved_cfg: Config,
    images: HashMap<String, Option<(TextureHandle, Vec2)>>,
    find: Option<Find>,
    about: bool,
    settings: bool,
    font_list: Vec<String>,
    font_filter: String,
    applied_font: (String, u16, u16, Lang),
    applied_gamma: f32,
    /// When the settings started to differ from the saved ones.
    cfg_changed_at: Option<f64>,
    error: Option<String>,
    print_job: Option<Receiver<Result<(), String>>>,
    close_queue: Vec<u64>,
    file_prompt: Option<(u64, bool)>,
    next_disk_check: f64,
    quit_after: bool,
    allow_quit: bool,
    rx: Receiver<Vec<PathBuf>>,
    syntax_worker: Option<highlight::Worker>,
    clipboard: Option<arboard::Clipboard>,
    v_pressed: bool,
    pasted: bool,
    scroll_to: Option<usize>,
    /// Editor scroll offset to apply on the next frame (drag-selection auto-scroll).
    scroll_pending: Option<f32>,
    editor_selection: Option<EditorSelection>,
    seen_selection: Option<(u64, (usize, usize))>,
    selection_tools: Option<SelectionTools>,
    title: String,
    recent: Vec<PathBuf>,
    browser: Option<files::Browser>,
    /// Name being typed in the "Create new" dialog of the file panel.
    new_file: Option<String>,
    new_page: Option<NewPage>,
    /// File waiting for delete confirmation.
    delete_file: Option<PathBuf>,
    /// Last text put into the primary selection (middle-click paste).
    primary: String,
    now: f64,
}

fn char_to_byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

fn byte_to_char(s: &str, b: usize) -> usize {
    s[..b].chars().count()
}

fn shortcut_label(m: Modifiers, k: Key) -> String {
    let mut s = String::new();
    if m.is_none() {
        return k.symbol_or_name().to_owned();
    }
    if m.command || m.ctrl {
        s += "Ctrl+";
    }
    if m.shift {
        s += "Shift+";
    }
    if m.alt {
        s += "Alt+";
    }
    if m.mac_cmd && m.ctrl {
        s += "Cmd+";
    }
    s + if k == Key::Backtick { "~" } else { k.symbol_or_name() }
}

fn save_all_mods() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers { ctrl: true, shift: true, mac_cmd: true, command: true, alt: false }
    } else {
        // egui does not report the Super/Meta key on Linux/Windows, so Alt stands in for it.
        Modifiers::COMMAND | Modifiers::SHIFT | Modifiers::ALT
    }
}

/// One line of the shortcut list in About.
struct AboutRow {
    what: &'static str,
    keys: String,
    syntax: &'static str,
    /// Shortcut-table actions this row documents (read by the test that keeps About complete).
    #[cfg_attr(not(test), allow(dead_code))]
    covers: Vec<Action>,
}

/// The shortcut list shown in About; keys come from `shortcuts()`, so they cannot drift apart.
fn about_rows(lang: Lang) -> Vec<AboutRow> {
    let t = |s| tr(lang, s);
    let table = shortcuts();
    let by = |what, actions: &[Action], syntax| {
        let keys = actions
            .iter()
            .flat_map(|a| table.iter().filter(move |(_, _, x)| x == a))
            .map(|(m, k, _)| shortcut_label(*m, *k))
            .collect::<Vec<_>>()
            .join(", ");
        AboutRow { what, keys, syntax, covers: actions.to_vec() }
    };
    let manual = |what, keys: &str, syntax| AboutRow { what, keys: keys.to_owned(), syntax, covers: vec![] };
    let h = |n| Action::Fmt(Fmt::Heading(n));
    let f = Action::Fmt;
    let mut rows = vec![
        by(t("New"), &[Action::New], ""),
        by(t("Open…"), &[Action::Open], ""),
        by(t("Save"), &[Action::Save], ""),
        by(t("Save as…"), &[Action::SaveAs], ""),
        by(t("Save all"), &[Action::SaveAll], ""),
        by(t("Export PDF…"), &[Action::ExportPdf], ""),
        by(t("Print…"), &[Action::Print], ""),
        by(t("Insert emoji…"), &[Action::Emoji], ""),
        by(t("Close"), &[Action::Close], ""),
        by(t("Close all"), &[Action::CloseAll], ""),
        by(t("Exit"), &[Action::Exit], ""),
        by(t("Undo"), &[Action::Undo], ""),
        by(t("Redo"), &[Action::Redo], ""),
        manual(t("Copy"), "Ctrl+C", ""),
        manual(t("Paste text or image"), "Ctrl+V", ""),
        by(t("Paste selection"), &[Action::PastePrimary], t("Middle click")),
        by(t("Find/Replace"), &[Action::Find], ""),
        by(t("Insert image…"), &[Action::Insert], ""),
        by(t("Files"), &[Action::Files], ""),
        by(t("View width"), &[Action::CycleWidth], ""),
        by(t("Preferences…"), &[Action::Preferences], ""),
        by(t("About"), &[Action::About], ""),
        by(t("Bold"), &[f(Fmt::Bold)], "**text**"),
        by(t("Italic"), &[f(Fmt::Italic)], "*text*"),
        by(t("Strikethrough"), &[f(Fmt::Strike)], "~~text~~"),
        by(t("Inline code"), &[f(Fmt::Code)], "`code`"),
        by(t("Highlight"), &[f(Fmt::Mark)], "==text=="),
        by(t("Link"), &[f(Fmt::Link)], "[text](url)"),
        by(t("New linked page"), &[Action::NewPage], "[text](file.md)"),
        by(t("Bullet list"), &[f(Fmt::Bullet)], "-"),
        by(t("Task (checkbox)"), &[f(Fmt::Task)], "- [ ]"),
        by(t("Quote"), &[f(Fmt::Quote)], ">"),
        by(t("Normal text"), &[h(0)], ""),
        manual(t("New list item"), "Enter", ""),
        manual(t("Indent list item"), "Tab, Shift+Tab", ""),
        manual(t("Font size"), t("Ctrl+scroll"), ""),
        manual(t("Open link or footnote"), "Ctrl+Click", ""),
    ];
    let mut heading = by(t("Heading 1–6"), &[h(1), h(2), h(3), h(4), h(5), h(6)], "#");
    heading.keys = "Ctrl+1 … 6".to_owned();
    rows.insert(rows.len() - 5, heading);
    rows
}

fn shortcuts() -> &'static [(Modifiers, Key, Action)] {
    static TABLE: OnceLock<Vec<(Modifiers, Key, Action)>> = OnceLock::new();
    TABLE.get_or_init(build_shortcuts)
}

/// Order matters: more specific modifier sets first (egui matches shortcuts "logically").
fn build_shortcuts() -> Vec<(Modifiers, Key, Action)> {
    use Action::*;
    let c = Modifiers::COMMAND;
    let cs = Modifiers::COMMAND | Modifiers::SHIFT;
    let mut v = vec![
        (Modifiers::NONE, Key::F2, CycleWidth),
        (save_all_mods(), Key::S, SaveAll),
        (cs, Key::S, SaveAs),
        (c, Key::S, Save),
        (cs, Key::E, ExportPdf),
        (c, Key::P, Print),
        (c, Key::Backtick, Emoji),
        (cs, Key::W, CloseAll),
        (c, Key::W, Close),
        (c, Key::O, Open),
        (c, Key::N, New),
        (c, Key::T, New),
        (cs, Key::I, Insert),
        (cs, Key::F, Files),
        (c, Key::F, Find),
        (cs, Key::Z, Redo),
        (c, Key::Z, Undo),
        (c, Key::Y, Redo),
        (Modifiers::SHIFT, Key::Insert, PastePrimary),
        (c, Key::Comma, Preferences),
        (Modifiers::NONE, Key::F1, About),
        (cs, Key::H, Fmt(self::Fmt::Mark)),
        (cs, Key::L, Fmt(self::Fmt::Bullet)),
        (cs, Key::Q, Fmt(self::Fmt::Quote)),
        (cs, Key::K, NewPage),
        (c, Key::Q, Exit),
        (c, Key::B, Fmt(self::Fmt::Bold)),
        (c, Key::I, Fmt(self::Fmt::Italic)),
        (c, Key::D, Fmt(self::Fmt::Strike)),
        (c, Key::E, Fmt(self::Fmt::Code)),
        (c, Key::K, Fmt(self::Fmt::Link)),
        (c, Key::L, Fmt(self::Fmt::Task)),
    ];
    let nums = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6];
    for (n, k) in nums.into_iter().enumerate() {
        v.push((c, k, Fmt(self::Fmt::Heading(n as u8))));
    }
    v
}

fn setup_style(ctx: &Context, cfg: &Config) {
    fonts::apply(ctx, &cfg.font, cfg.weight, cfg.bold_weight, cfg.lang);
    ctx.all_styles_mut(|s| {
        s.spacing.button_padding = vec2(8.0, 4.0);
        s.spacing.item_spacing = vec2(8.0, 5.0);
        s.spacing.menu_margin = Margin::same(8);
        s.spacing.window_margin = Margin::same(16);
    });

    let mut v = egui::Visuals::dark();
    let bg = md::BG;
    v.panel_fill = bg;
    v.extreme_bg_color = Color32::from_rgb(0x2e, 0x2e, 0x2e);
    v.window_fill = CHROME;
    v.selection.bg_fill = md::ACCENT.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0, md::ACCENT);
    v.hyperlink_color = md::ACCENT;
    v.text_cursor.stroke = Stroke::new(2.0, md::ACCENT);
    // egui's dark-mode default (2c - c²) visibly emboldens light text; a plain gamma is closer to browsers.
    v.text_options.color_transfer_function = FontColorTransferFunction::Gamma(cfg.gamma);
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals(v);
}

/// Cache key of an image source: its resolved path, or a hash for `data:` URIs.
fn image_key(dir: Option<&Path>, src: &str) -> Option<String> {
    if src.starts_with("data:") {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        src.hash(&mut h);
        return Some(format!("data:{:x}", h.finish()));
    }
    resolve(dir, src).map(|p| p.to_string_lossy().into_owned())
}

/// Larger images are scaled down before they become textures (GPUs limit the texture size).
const MAX_TEXTURE_SIDE: u32 = 4096;

/// Decodes an image, refusing absurd sizes up front (a tiny file can claim to be gigapixels).
fn decode_image<R: std::io::BufRead + std::io::Seek>(reader: image::ImageReader<R>) -> Option<image::DynamicImage> {
    let mut reader = reader.with_guessed_format().ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    reader.limits(limits);
    reader.decode().ok()
}

fn load_image(ctx: &Context, dir: Option<&Path>, src: &str) -> Option<(TextureHandle, Vec2)> {
    let img = match src.strip_prefix("data:") {
        Some(data) => {
            let bytes = base64::decode(data.split_once(',')?.1)?;
            decode_image(image::ImageReader::new(std::io::Cursor::new(bytes)))?
        }
        None => decode_image(image::ImageReader::open(resolve(dir, src)?).ok()?)?,
    };
    let max_side = MAX_TEXTURE_SIDE.min(ctx.input(|i| i.max_texture_side).max(1) as u32);
    let img =
        if img.width().max(img.height()) > max_side { img.thumbnail(max_side, max_side) } else { img }.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ctx.load_texture(image_key(dir, src)?, ci, Default::default());
    Some((tex, vec2(size[0] as f32, size[1] as f32)))
}

fn resolve(dir: Option<&Path>, src: &str) -> Option<PathBuf> {
    if src.contains("://") {
        return None;
    }
    let decoded = md::decode_path(src)?;
    let p = Path::new(&decoded);
    if p.is_absolute() { Some(p.to_path_buf()) } else { dir.map(|d| d.join(p)) }
}

fn fit_width(rgba: image::RgbaImage) -> image::RgbaImage {
    if rgba.width() <= MAX_IMG_W {
        return rgba;
    }
    let h = (rgba.height() as u64 * MAX_IMG_W as u64 / rgba.width() as u64).max(1) as u32;
    image::imageops::resize(&rgba, MAX_IMG_W, h, image::imageops::FilterType::Lanczos3)
}

/// Encodes a pasted image as a `data:` URI: JPEG at 85% quality, at most `MAX_IMG_W` wide,
/// transparency flattened onto white.
fn jpeg_data_uri(rgba: image::RgbaImage) -> Result<String, String> {
    let rgba = fit_width(rgba);
    let rgb = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let blend = |c: u8| ((c as u32 * a as u32 + 255 * (255 - a as u32)) / 255) as u8;
        image::Rgb([blend(r), blend(g), blend(b)])
    });
    let mut buf = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 85).encode_image(&rgb).map_err(|e| e.to_string())?;
    Ok(format!("data:image/jpeg;base64,{}", base64::encode(&buf)))
}

/// Saves a pasted image next to the document as WebP, scaled down to `MAX_IMG_W`. Returns the file name.
fn save_webp(rgba: image::RgbaImage, dir: &Path, stem: &str) -> Result<String, String> {
    let rgba = fit_width(rgba);
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut name = format!("{stem}-{secs}.webp");
    let mut k = 1;
    while dir.join(&name).exists() {
        name = format!("{stem}-{secs}-{k}.webp");
        k += 1;
    }
    rgba.save_with_format(dir.join(&name), image::ImageFormat::WebP).map_err(|e| format!("{name}: {e}"))?;
    Ok(name)
}

/// The scheme of a URL such as `https://…` or `mailto:…` (not a Windows drive letter like `C:`).
fn url_scheme(url: &str) -> Option<&str> {
    let (scheme, _) = url.split_once(':')?;
    let valid = scheme.len() > 1 && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// Whether opening the file with the system's default handler might run code.
fn runs_code(path: &Path) -> bool {
    const RISKY: &[&str] = &[
        "exe", "com", "bat", "cmd", "msi", "scr", "pif", "lnk", "ps1", "vbs", "vbe", "js", "jse", "wsf", "hta", "reg",
        "jar", "sh", "bash", "desktop", "appimage", "run", "bin", "app", "deb", "rpm", "dll", "cpl",
    ];
    let risky_name = path.extension().is_some_and(|e| RISKY.iter().any(|r| e.eq_ignore_ascii_case(r)));
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    #[cfg(not(unix))]
    let executable = false;
    risky_name || executable
}

fn md_link_target(p: &str) -> String {
    if p.contains(' ') { format!("<{p}>") } else { p.to_owned() }
}

fn relative_to(path: &Path, dir: Option<&Path>) -> String {
    let rel = dir.and_then(|d| path.strip_prefix(d).ok()).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

impl App {
    pub fn new(cc: &eframe::CreationContext, files: Vec<PathBuf>, rx: Receiver<Vec<PathBuf>>) -> Self {
        let cfg = Config::load();
        setup_style(&cc.egui_ctx, &cfg);
        let mut app = Self {
            docs: vec![],
            active: 0,
            next_id: 0,
            saved_cfg: cfg.clone(),
            cfg,
            images: HashMap::new(),
            find: None,
            about: false,
            settings: false,
            font_list: vec![],
            font_filter: String::new(),
            applied_font: (String::new(), 0, 0, Lang::English),
            applied_gamma: 0.0,
            cfg_changed_at: None,
            error: None,
            print_job: None,
            close_queue: vec![],
            file_prompt: None,
            next_disk_check: 0.0,
            quit_after: false,
            allow_quit: false,
            rx,
            syntax_worker: None,
            clipboard: arboard::Clipboard::new().ok(),
            v_pressed: false,
            pasted: false,
            scroll_to: None,
            scroll_pending: None,
            editor_selection: None,
            seen_selection: None,
            selection_tools: None,
            title: String::new(),
            recent: Config::load_recent(),
            browser: None,
            new_file: None,
            new_page: None,
            delete_file: None,
            primary: String::new(),
            now: 0.0,
        };
        app.applied_font = (app.cfg.font.clone(), app.cfg.weight, app.cfg.bold_weight, app.cfg.lang);
        app.applied_gamma = app.cfg.gamma;
        for f in files {
            app.open_path(f);
        }
        if app.docs.is_empty() {
            app.new_doc();
        }
        app
    }

    /// Writes changed settings after they stayed unchanged for half a second (sliders and Ctrl+scroll
    /// change them every frame); `on_exit` writes what is left.
    fn save_config_soon(&mut self, ctx: &Context) {
        if self.cfg == self.saved_cfg {
            self.cfg_changed_at = None;
            return;
        }
        let since = *self.cfg_changed_at.get_or_insert(self.now);
        if self.now - since >= 0.5 {
            self.cfg.save();
            self.saved_cfg = self.cfg.clone();
            self.cfg_changed_at = None;
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn t(&self, s: &'static str) -> &'static str {
        tr(self.cfg.lang, s)
    }

    fn new_doc(&mut self) {
        self.next_id += 1;
        self.docs.push(Doc::new(self.next_id, None, String::new()));
        self.active = self.docs.len() - 1;
    }

    /// Opens `p` in a tab (or switches to it). Binary and very large files are refused.
    fn open_path(&mut self, p: PathBuf) {
        let p = p.canonicalize().unwrap_or(p);
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(&p)) {
            self.active = i;
            if self.file_prompt.is_none() {
                if self.docs[i].dirty() {
                    self.file_prompt = Some((self.docs[i].id, false));
                } else {
                    self.check_disk(i);
                }
            }
            return;
        }
        if let Some(why) = files::open_warning(&p) {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.error = Some(format!("{}: {name}", self.t(why)));
            return;
        }
        let (text, crlf) = match std::fs::read_to_string(&p) {
            Ok(t) => (t.replace("\r\n", "\n"), t.contains("\r\n")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
            Err(e) => {
                self.error = Some(format!("{}: {e}", p.display()));
                return;
            }
        };
        self.add_recent(&p);
        self.next_id += 1;
        let mut doc = Doc::new(self.next_id, Some(p), text);
        doc.crlf = crlf;
        let reuse = self.docs.len() == 1 && self.docs[0].path.is_none() && self.docs[0].text.is_empty();
        if reuse {
            self.docs[0] = doc;
            self.active = 0;
        } else {
            self.docs.push(doc);
            self.active = self.docs.len() - 1;
        }
    }

    /// Compare contents rather than timestamps: atomic replacements and coarse mtimes are common.
    fn check_disk(&mut self, i: usize) {
        let doc = &mut self.docs[i];
        let Some(path) = &doc.path else { return };
        if files::open_warning(path).is_some() {
            return;
        }
        if let Ok(raw) = std::fs::read_to_string(path) {
            let hash = disk_hash(&raw.replace("\r\n", "\n"));
            if hash != doc.disk_hash {
                doc.disk_hash = hash;
                self.file_prompt = Some((doc.id, true));
            }
        }
    }

    fn reload_doc(&mut self, i: usize) -> bool {
        let Some(path) = self.docs[i].path.clone() else { return false };
        if let Some(why) = files::open_warning(&path) {
            self.error = Some(self.t(why).into());
            return false;
        }
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let doc = &mut self.docs[i];
                doc.checkpoint(self.now);
                doc.text = raw.replace("\r\n", "\n");
                doc.crlf = raw.contains("\r\n");
                doc.saved = doc.text.clone();
                doc.disk_hash = disk_hash(&doc.text);
                doc.checkpoint(self.now);
                doc.cache = None;
                doc.stats.set(None);
                true
            }
            Err(e) => {
                self.error = Some(format!("{}: {e}", path.display()));
                false
            }
        }
    }

    fn save(&mut self, i: usize, ask: bool) -> bool {
        let doc = &mut self.docs[i];
        if ask || doc.path.is_none() {
            let name = doc
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("{}.md", doc.stem(self.cfg.lang)));
            // A Markdown-only native filter may append .md even to an explicit .sh name.
            // Let the user enter any extension; apply the Markdown default below only if absent.
            let mut dlg = rfd::FileDialog::new().add_filter(tr(self.cfg.lang, "Files"), &["*"]).set_file_name(name);
            if let Some(d) = doc.dir() {
                dlg = dlg.set_directory(d);
            }
            let Some(mut p) = dlg.save_file() else {
                return false;
            };
            if p.extension().is_none() {
                p.set_extension("md");
            }
            doc.path = Some(p.clone());
            doc.set_syntax(doc.syntax_mode);
            self.add_recent(&p);
        }
        let doc = &mut self.docs[i];
        let path = doc.path.clone().unwrap();
        let data = if doc.crlf { doc.text.replace('\n', "\r\n") } else { doc.text.clone() };
        match std::fs::write(&path, data) {
            Ok(()) => {
                doc.saved = doc.text.clone();
                doc.disk_hash = disk_hash(&doc.text);
                true
            }
            Err(e) => {
                self.error = Some(format!("{}: {e}", path.display()));
                false
            }
        }
    }

    /// Creates the file named in the "Create new" dialog in the file panel's folder and opens it.
    fn create_file(&mut self) {
        let (Some(name), Some(browser)) = (self.new_file.clone(), self.browser.as_mut()) else { return };
        let Some(name) = files::file_name(&name) else { return };
        self.new_file = None;
        let mut path = browser.dir.join(name);
        if path.extension().is_none() {
            path.set_extension("md");
        }
        browser.reload();
        match files::create_empty(&path) {
            Ok(()) => self.open_path(path),
            Err(error) => self.error = Some(error),
        }
    }

    /// Deletes a file from the file panel, into the trash or permanently. Its tab is closed if unchanged.
    fn delete(&mut self, path: &Path, trash: bool) {
        let result = if trash { move_to_trash(path) } else { std::fs::remove_file(path).map_err(|e| e.to_string()) };
        if let Err(e) = result {
            self.error = Some(format!("{}: {e}", path.display()));
            return;
        }
        if let Some(b) = self.browser.as_mut() {
            b.reload();
        }
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(path) && !d.dirty()) {
            self.remove_doc(i);
        }
    }

    fn add_recent(&mut self, p: &Path) {
        self.recent.retain(|r| r != p);
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(RECENT);
        Config::save_recent(&self.recent);
    }

    /// Ends the current undo step of the active document (call around programmatic edits).
    fn checkpoint(&mut self, ctx: &Context) {
        self.renumber_lists(ctx);
        let now = self.now;
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.checkpoint(now);
        }
    }

    fn undo_redo(&mut self, ctx: &Context, redo: bool) {
        let now = self.now;
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        if let Some(c) = doc.step(now, redo) {
            self.set_selection(ctx, c, c);
        }
    }

    /// Normalize changed Markdown before recording history, mapping both cursor endpoints.
    fn renumber_lists(&mut self, ctx: &Context) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        if doc.syntax.is_some() || doc.text == doc.prev {
            return;
        }
        let edits = numbered_edits(&doc.text);
        if edits.is_empty() {
            return;
        }
        let id = doc.edit_id();
        let mut state = TextEditState::load(ctx, id).unwrap_or_default();
        let mut range = state.cursor.char_range();
        // Translate sorted byte offsets in one pass instead of rescanning the document per item.
        let (mut byte, mut chars) = (0, 0);
        let edits: Vec<_> = edits
            .into_iter()
            .map(|(start, end, replacement)| {
                chars += doc.text[byte..start].chars().count();
                let a = chars;
                chars += end - start; // Replaced number markers contain ASCII digits only.
                byte = end;
                (start, end, a, chars, replacement)
            })
            .collect();
        for (start, end, a, b, replacement) in edits.into_iter().rev() {
            if let Some(r) = range.as_mut() {
                for c in [&mut r.primary, &mut r.secondary] {
                    c.index.0 = if c.index.0 >= b {
                        c.index.0 - (b - a) + replacement.len()
                    } else if c.index.0 > a {
                        a + replacement.len()
                    } else {
                        c.index.0
                    };
                }
            }
            doc.text.replace_range(start..end, &replacement);
        }
        state.cursor.set_char_range(range);
        state.store(ctx, id);
        ctx.request_repaint();
    }

    /// Enter continues a list or quote (or ends it on an empty item); Tab / Shift+Tab indent list items.
    fn list_keys(&mut self, ctx: &Context) {
        let Some(doc) = self.docs.get(self.active) else { return };
        if doc.syntax.is_some() || !ctx.memory(|m| m.has_focus(doc.edit_id())) {
            return;
        }
        let Some((a, b)) = self.selection(ctx) else { return };
        let text = &doc.text;
        let ba = char_to_byte(text, a);
        let ls = text[..ba].rfind('\n').map_or(0, |i| i + 1);
        let le = text[ba..].find('\n').map_or(text.len(), |i| ba + i);
        let parsed = list_prefix(&text[ls..le]).or_else(|| {
            let line = &text[ls..le];
            if !line.starts_with([' ', '\t']) {
                return None;
            }
            text[..ls].lines().rev().take_while(|l| !l.trim().is_empty()).find_map(|l| {
                let (prefix, next, _) = list_prefix(l)?;
                (!next.is_empty()).then(|| (prefix, next, line.len() - line.trim_start().len()))
            })
        });
        let Some((prefix, next, rest)) = parsed else { return };
        if !next.is_empty() && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Enter)) {
            self.insert(ctx, &format!("\n{prefix}{}", " ".repeat(markdown::INDENT_SPACES)));
            return;
        }
        let empty = text[ls + rest..le].trim().is_empty();
        let lsc = byte_to_char(text, ls);
        let shift_tab = !next.is_empty() && ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
        let tab = !shift_tab && !next.is_empty() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab));
        let enter = !tab && !shift_tab && a == b && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
        if enter && !empty {
            self.insert(ctx, &format!("\n{prefix}{next}"));
            return;
        }
        let (range, with, cursor) = if enter {
            // Enter on an empty item ends the list.
            (ls..le, String::new(), lsc)
        } else if tab {
            (ls..ls, "\t".to_owned(), a + 1)
        } else if shift_tab {
            let line = &text[ls..le];
            let n = if line.starts_with('\t') {
                1
            } else {
                line.bytes().take(markdown::INDENT_SPACES).take_while(|c| *c == b' ').count()
            };
            (ls..ls + n, String::new(), a.saturating_sub(n).max(lsc))
        } else {
            return;
        };
        self.checkpoint(ctx);
        self.docs[self.active].text.replace_range(range, &with);
        self.set_selection(ctx, cursor, cursor);
        self.checkpoint(ctx);
    }

    fn begin_new_page(&mut self, ctx: &Context) {
        let Some(doc) = self.docs.get(self.active).filter(|d| d.syntax.is_none()) else { return };
        let end = doc.text.chars().count();
        let selection = self.selection(ctx).unwrap_or((end, end));
        let name = doc.text[char_to_byte(&doc.text, selection.0)..char_to_byte(&doc.text, selection.1)].to_owned();
        let dir =
            doc.dir().or_else(|| self.browser.as_ref().map(|b| b.dir.clone())).or_else(|| std::env::current_dir().ok());
        let Some(dir) = dir else { return };
        self.new_page = Some(NewPage {
            name,
            doc_id: doc.id,
            selection,
            original: doc.text.clone(),
            dir,
            focus: true,
            error: None,
        });
    }

    fn create_linked_page(&mut self, ctx: &Context) -> Result<(), String> {
        let Some(page) = &self.new_page else { return Ok(()) };
        let name = files::file_name(&page.name).ok_or_else(|| self.t("Invalid file name").to_owned())?;
        let file =
            if name.ends_with(".md") || name.ends_with(".markdown") { name.to_owned() } else { format!("{name}.md") };
        let Some(index) = self.docs.iter().position(|d| d.id == page.doc_id && d.text == page.original) else {
            return Err(self.t("Unsaved changes").to_owned());
        };
        let (a, b) = page.selection;
        let selected = &page.original[char_to_byte(&page.original, a)..char_to_byte(&page.original, b)];
        let label = if selected.is_empty() { name } else { selected };
        let label = label.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]");
        let destination: String = file
            .bytes()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.') {
                    (c as char).to_string()
                } else {
                    format!("%{c:02X}")
                }
            })
            .collect();
        let link = format!("[{label}]({destination})");
        let path = page.dir.join(&file);
        files::create_empty(&path)?;
        self.active = index;
        self.set_selection(ctx, a, b);
        self.insert(ctx, &link);
        self.new_page = None;
        if let Some(browser) = &mut self.browser {
            browser.reload();
        }
        Ok(())
    }

    fn has_modal(&self) -> bool {
        self.file_prompt.is_some()
            || !self.close_queue.is_empty()
            || self.error.is_some()
            || self.about
            || self.settings
            || self.new_page.is_some()
            || self.new_file.is_some()
            || self.delete_file.is_some()
    }

    fn formatting_grid(&self, ui: &mut Ui, acts: &mut Vec<Action>) {
        for row in formatting_buttons().chunks(TOOL_COLUMNS) {
            ui.horizontal(|ui| {
                for &(icon, label, action) in row {
                    let shortcut = shortcuts()
                        .iter()
                        .find(|(_, _, a)| *a == action)
                        .map(|(m, k, _)| shortcut_label(*m, *k))
                        .unwrap_or_default();
                    let tooltip = format!("{}  {shortcut}", self.t(label));
                    if formatting_button(ui, icon, action).on_hover_text(tooltip).clicked() {
                        acts.push(action);
                    }
                }
            });
        }
    }

    fn dismiss_selection_tools(&mut self, ctx: &Context) {
        if self.has_modal() {
            self.selection_tools = None;
            return;
        }
        if let Some(tools) = &self.selection_tools {
            let outside = ctx.input(|i| {
                i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !tools.rect.contains(p))
            });
            let escape = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            if outside || escape {
                let selection = tools.selection;
                let restore = escape || tools.emoji;
                self.selection_tools = None;
                if restore && self.docs.get(self.active).is_some_and(|doc| doc.id == selection.doc_id) {
                    self.set_selection(ctx, selection.range.0, selection.range.1);
                }
            }
        }
    }

    fn selection_popup(&mut self, ctx: &Context, acts: &mut Vec<Action>) {
        let Some(selection) = self.editor_selection else {
            self.selection_tools = None;
            self.seen_selection = None;
            return;
        };
        let key = selection.key();
        if self.has_modal() {
            self.selection_tools = None;
            self.seen_selection = Some(key);
            return;
        }
        if self.selection_tools.as_ref().is_some_and(|tools| tools.selection.doc_id != selection.doc_id) {
            self.selection_tools = None;
        }
        if self.selection_tools.as_ref().is_some_and(|tools| !tools.emoji)
            && (selection.range.0 == selection.range.1 || self.docs[self.active].syntax.is_some())
        {
            self.selection_tools = None;
        }
        let dragging = ctx.input(|i| i.pointer.any_down());
        if dragging && self.seen_selection != Some(key) {
            // A fresh drag can end on exactly the previously dismissed range.
            // Reopen only on release, even when its final endpoints are unchanged.
            self.seen_selection = None;
        }
        if !dragging && self.seen_selection != Some(key) {
            self.seen_selection = Some(key);
            if self.selection_tools.as_ref().is_none_or(|tools| !tools.emoji) {
                self.selection_tools = (selection.range.0 != selection.range.1
                    && self.docs[self.active].syntax.is_none())
                .then_some(SelectionTools {
                    selection,
                    emoji: false,
                    query: String::new(),
                    focus_search: false,
                    rect: Rect::NOTHING,
                });
            }
        }
        let Some(tools) = &mut self.selection_tools else { return };
        tools.selection = selection;
        let emoji = tools.emoji;
        let mut query = std::mem::take(&mut tools.query);
        let focus_search = std::mem::take(&mut tools.focus_search);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(8).fill(CHROME);
        let size = if emoji { emoji_content_size() } else { tool_content_size() };
        let total = size + frame.total_margin().sum();
        // Prefer the bottom-left corner immediately above the selection's end;
        // shift just enough to keep the whole window inside a small viewport.
        let screen = ctx.content_rect().shrink(4.0);
        let anchor = pos2(
            selection.anchor.x.clamp(screen.left(), (screen.right() - total.x).max(screen.left())),
            (selection.anchor.y - 6.0).clamp((screen.top() + total.y).min(screen.bottom()), screen.bottom()),
        );
        let mut picked = None;
        let action_count = acts.len();
        let response = egui::Area::new(Id::new("selection_tools"))
            .order(egui::Order::Foreground)
            .pivot(Align2::LEFT_TOP)
            .fixed_pos(anchor - vec2(0.0, total.y))
            .default_size(total)
            .movable(false)
            .fade_in(false)
            .show(ctx, |ui| {
                frame.show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    ui.spacing_mut().item_spacing = Vec2::splat(TOOL_GAP);
                    if emoji {
                        let search = ui.add_sized(
                            vec2(size.x, EMOJI_SEARCH_HEIGHT),
                            TextEdit::singleline(&mut query)
                                .return_key(None)
                                .id(Id::new(EMOJI_SEARCH_ID))
                                .hint_text(self.t("Search emoji…"))
                                .desired_width(f32::INFINITY),
                        );
                        if focus_search {
                            search.request_focus();
                        }
                        let chars = crate::emoji::search(&query, self.cfg.lang);
                        let grid_height = size.y - EMOJI_SEARCH_HEIGHT - TOOL_GAP;
                        ui.spacing_mut().scroll.floating = true;
                        ui.spacing_mut().scroll.bar_width = 3.0;
                        ui.spacing_mut().scroll.bar_inner_margin = 0.0;
                        ui.spacing_mut().scroll.bar_outer_margin = 0.0;
                        // Put the narrow overlay scrollbar in the frame padding,
                        // leaving identical left/right grid margins in both modes.
                        let scroll_rect =
                            Rect::from_min_size(ui.next_widget_position(), vec2(size.x + 5.0, grid_height));
                        let mut scroll = ScrollArea::vertical().id_salt("emoji_palette");
                        if search.changed() {
                            scroll = scroll.vertical_scroll_offset(0.0);
                        }
                        scroll
                            .scroll_bar_rect(scroll_rect)
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                            .max_height(grid_height)
                            .min_scrolled_height(grid_height)
                            .auto_shrink([false, false])
                            .show_rows(ui, EMOJI_BUTTON.y, chars.len().div_ceil(TOOL_COLUMNS), |ui, rows| {
                                if chars.is_empty() {
                                    ui.label(self.t("No emoji found"));
                                }
                                for row in rows {
                                    ui.horizontal(|ui| {
                                        for ch in chars.iter().skip(row * TOOL_COLUMNS).take(TOOL_COLUMNS) {
                                            let response = tool_button(ui, EMOJI_BUTTON);
                                            crate::emoji::paint_character(
                                                ui.ctx(),
                                                ui.painter(),
                                                *ch,
                                                response.rect.shrink(4.0),
                                            );
                                            let response =
                                                response.on_hover_text(crate::emoji::name(*ch, self.cfg.lang));
                                            response.widget_info(|| {
                                                egui::WidgetInfo::labeled(
                                                    egui::WidgetType::Button,
                                                    ui.is_enabled(),
                                                    ch.to_string(),
                                                )
                                            });
                                            if response.clicked() {
                                                picked = Some(*ch);
                                            }
                                        }
                                    });
                                }
                            });
                    } else {
                        self.formatting_grid(ui, acts);
                    }
                });
            });
        let tools = self.selection_tools.as_mut().unwrap();
        tools.rect = response.response.rect;
        tools.query = query;
        if picked.is_some() || acts.len() != action_count {
            self.set_selection(ctx, selection.range.0, selection.range.1);
        }
        if let Some(ch) = picked {
            self.insert(ctx, &ch.to_string());
            self.selection_tools = None;
            // Insertion collapses the selection; don't recreate formatting on its old range.
            self.seen_selection = self.selection(ctx).map(|range| (selection.doc_id, range));
        }
    }

    fn primary_text(&mut self) -> Option<String> {
        let cb = self.clipboard.as_mut()?;
        #[cfg(target_os = "linux")]
        {
            use arboard::{GetExtLinux, LinuxClipboardKind};
            if let Ok(t) = cb.get().clipboard(LinuxClipboardKind::Primary).text() {
                return Some(t);
            }
        }
        cb.get_text().ok()
    }

    /// Mirrors the editor selection into the system primary selection (Linux).
    fn sync_primary(&mut self, ctx: &Context) {
        if ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        let Some((a, b)) = self.selection(ctx).filter(|(a, b)| a != b) else {
            return;
        };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let sel = &doc.text[char_to_byte(&doc.text, a)..char_to_byte(&doc.text, b)];
        if sel == self.primary {
            return;
        }
        self.primary = sel.to_owned();
        #[cfg(target_os = "linux")]
        if let Some(cb) = self.clipboard.as_mut() {
            use arboard::{LinuxClipboardKind, SetExtLinux};
            let _ = cb.set().clipboard(LinuxClipboardKind::Primary).text(self.primary.clone());
        }
    }

    /// Quits, asking about each document with unsaved changes first.
    fn exit(&mut self, ctx: &Context) {
        self.close_queue = self.docs.iter().filter(|d| d.dirty()).map(|d| d.id).collect();
        if self.close_queue.is_empty() {
            self.allow_quit = true;
            ctx.send_viewport_cmd(ViewportCommand::Close);
        } else {
            self.quit_after = true;
        }
    }

    fn close(&mut self, ids: Vec<u64>) {
        for id in ids {
            match self.docs.iter().position(|d| d.id == id) {
                Some(i) if self.docs[i].dirty() => self.close_queue.push(id),
                Some(i) => self.remove_doc(i),
                None => {}
            }
        }
    }

    fn remove_doc(&mut self, i: usize) {
        self.docs.remove(i);
        if self.active >= i && self.active > 0 {
            self.active -= 1;
        }
    }

    fn selection(&self, ctx: &Context) -> Option<(usize, usize)> {
        let doc = self.docs.get(self.active)?;
        let r = TextEditState::load(ctx, doc.edit_id())?.cursor.char_range()?;
        let (a, b) = (r.primary.index.0, r.secondary.index.0);
        Some((a.min(b), a.max(b)))
    }

    fn set_selection(&self, ctx: &Context, a: usize, b: usize) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let id = doc.edit_id();
        let mut st = TextEditState::load(ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
        st.store(ctx, id);
        ctx.memory_mut(|m| m.request_focus(id));
    }

    /// Replaces the selection (or inserts at the cursor) with `s`.
    fn insert(&mut self, ctx: &Context, s: &str) {
        self.checkpoint(ctx);
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        let n = doc.text.chars().count();
        let (a, b) = self.selection(ctx).unwrap_or((n, n));
        let doc = &mut self.docs[self.active];
        let (ba, bb) = (char_to_byte(&doc.text, a), char_to_byte(&doc.text, b));
        doc.text.replace_range(ba..bb, s);
        let c = a + s.chars().count();
        self.set_selection(ctx, c, c);
        self.checkpoint(ctx);
    }

    fn apply_fmt(&mut self, ctx: &Context, f: Fmt) {
        self.checkpoint(ctx);
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let n = doc.text.chars().count();
        let (a, b) = self.selection(ctx).unwrap_or((n, n));
        let text = &mut self.docs[self.active].text;
        let (ba, bb) = (char_to_byte(text, a), char_to_byte(text, b));
        let wrap = match f {
            Fmt::Bold => Some("**"),
            Fmt::Italic => Some("*"),
            Fmt::Strike => Some("~~"),
            Fmt::Code => Some("`"),
            Fmt::Mark => Some("=="),
            _ => None,
        };
        let (na, nb) = if let Some(m) = wrap {
            let ml = m.len();
            let sel = &text[ba..bb];
            if text[..ba].ends_with(m) && text[bb..].starts_with(m) {
                text.replace_range(bb..bb + ml, "");
                text.replace_range(ba - ml..ba, "");
                (a - ml, b - ml)
            } else if sel.len() >= 2 * ml && sel.starts_with(m) && sel.ends_with(m) {
                text.replace_range(bb - ml..bb, "");
                text.replace_range(ba..ba + ml, "");
                (a, b - 2 * ml)
            } else {
                text.insert_str(bb, m);
                text.insert_str(ba, m);
                (a + ml, b + ml)
            }
        } else if f == Fmt::Link {
            let sel = text[ba..bb].to_owned();
            if sel.contains("://") {
                text.replace_range(ba..bb, &format!("[]({sel})"));
                (a + 1, a + 1)
            } else {
                text.replace_range(ba..bb, &format!("[{sel}]()"));
                let c = b + 3;
                (c, c)
            }
        } else {
            // Line-level formats act on every line touched by the selection.
            let ls = text[..ba].rfind('\n').map_or(0, |i| i + 1);
            let le = text[bb..].find('\n').map_or(text.len(), |i| bb + i);
            let lines: Vec<String> = text[ls..le].split('\n').map(|l| toggle_line(l, f)).collect();
            let new = lines.join("\n");
            let delta = new.chars().count() as isize - text[ls..le].chars().count() as isize;
            text.replace_range(ls..le, &new);
            let lsc = byte_to_char(text, ls);
            let fix = |c: usize| (c as isize + delta).max(lsc as isize) as usize;
            if lines.len() == 1 { (fix(a), fix(b)) } else { (lsc, lsc + new.chars().count()) }
        };
        self.set_selection(ctx, na, nb);
        self.checkpoint(ctx);
    }

    fn find_next(&mut self, ctx: &Context) {
        let Some(f) = &self.find else { return };
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        if f.query.is_empty() {
            return;
        }
        let from = self.selection(ctx).map_or(0, |(_, b)| char_to_byte(&doc.text, b));
        let q = f.query.clone();
        let hit = doc.text[from..].find(&q).map(|i| i + from).or_else(|| doc.text.find(&q));
        let found = hit.map(|bi| {
            let a = byte_to_char(&doc.text, bi);
            (a, a + q.chars().count())
        });
        if let Some((a, b)) = found {
            let id = doc.edit_id();
            let mut st = TextEditState::load(ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(a), CCursor::new(b))));
            st.store(ctx, id);
            self.scroll_to = Some(a);
        }
        let f = self.find.as_mut().unwrap();
        f.hl = found;
        f.none = found.is_none();
    }

    fn replace_one(&mut self, ctx: &Context) {
        let Some(f) = &self.find else { return };
        let (q, r) = (f.query.clone(), f.replace.clone());
        if let (Some((a, b)), Some(doc)) = (self.selection(ctx), self.docs.get(self.active)) {
            let (ba, bb) = (char_to_byte(&doc.text, a), char_to_byte(&doc.text, b));
            if !q.is_empty() && doc.text[ba..bb] == q {
                self.insert(ctx, &r);
            }
        }
        self.find_next(ctx);
    }

    fn replace_all(&mut self, ctx: &Context) {
        let Some(f) = &self.find else { return };
        if f.query.is_empty() || self.docs.get(self.active).is_none() {
            return;
        }
        let (query, replacement) = (f.query.clone(), f.replace.clone());
        self.checkpoint(ctx);
        let doc = &mut self.docs[self.active];
        doc.text = doc.text.replace(&query, &replacement);
        self.checkpoint(ctx);
        self.find.as_mut().unwrap().hl = None;
    }

    fn paste_image(&mut self, ctx: &Context) -> bool {
        let Some(img) = self.clipboard.as_mut().and_then(|c| c.get_image().ok()) else {
            return false;
        };
        if self.docs.is_empty() {
            self.new_doc();
        }
        let Some(rgba) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) else {
            return true;
        };
        if self.cfg.embed_images {
            match jpeg_data_uri(rgba) {
                Ok(uri) => self.insert(ctx, &format!("![]({uri})")),
                Err(e) => self.error = Some(e),
            }
            return true;
        }
        if self.docs[self.active].path.is_none() && !self.save(self.active, true) {
            self.error = Some(self.t("Save the file first to paste images.").to_owned());
            return true;
        }
        let doc = &self.docs[self.active];
        let dir = doc.dir().unwrap_or_default();
        let name = match save_webp(rgba, &dir, &doc.stem(self.cfg.lang)) {
            Ok(name) => name,
            Err(e) => {
                self.error = Some(e);
                return true;
            }
        };
        self.insert(ctx, &format!("![]({})", md_link_target(&name)));
        true
    }

    fn insert_image_file(&mut self, ctx: &Context) {
        let mut dlg = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"]);
        let dir = self.docs.get(self.active).and_then(Doc::dir);
        if let Some(d) = &dir {
            dlg = dlg.set_directory(d);
        }
        if let Some(p) = dlg.pick_file() {
            if self.docs.is_empty() {
                self.new_doc();
            }
            let target = md_link_target(&relative_to(&p, dir.as_deref()));
            self.insert(ctx, &format!("![]({target})"));
        }
    }

    /// Ctrl+click on a footnote: a reference (`id`) jumps to its definition, a definition (`id:`)
    /// jumps back to the first reference.
    fn jump_footnote(&mut self, ctx: &Context, id: &str) {
        let Some(doc) = self.docs.get(self.active) else { return };
        let text = &doc.text;
        let (id, to_reference) = id.strip_suffix(':').map_or((id, false), |i| (i, true));
        let target = if to_reference {
            let marker = format!("[^{id}]");
            text.match_indices(&marker).map(|(i, _)| i).find(|i| !text[i + marker.len()..].starts_with(':'))
        } else {
            let marker = format!("[^{id}]:");
            text.match_indices(&marker).map(|(i, _)| i).find(|&i| i == 0 || text[..i].ends_with('\n'))
        };
        if let Some(b) = target {
            let ci = byte_to_char(text, b);
            self.set_selection(ctx, ci, ci);
            self.scroll_to = Some(ci);
        }
    }

    /// Ctrl+click on a link. Only web and mail links are handed to the system, and local files that
    /// could run code (programs, scripts, launchers) are never opened this way.
    fn open_link(&mut self, url: &str) {
        let dir = self.docs.get(self.active).and_then(Doc::dir);
        if let Some(name) = url.strip_prefix("[[") {
            let name = name.split(['|', '#']).next().unwrap_or(name);
            let file = if Path::new(name).extension().is_some() { name.to_owned() } else { format!("{name}.md") };
            if let Some(p) = resolve(dir.as_deref(), &file) {
                self.open_path(p);
            }
        } else if let Some(scheme) = url_scheme(url) {
            if matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https" | "mailto") {
                let _ = open::that(url);
            }
        } else if let Some(p) = resolve(dir.as_deref(), url.split('#').next().unwrap_or(url)) {
            if p.extension().is_some_and(|e| e == "md" || e == "markdown") {
                self.open_path(p);
            } else if !runs_code(&p) {
                let _ = open::that(p);
            }
        }
    }

    fn run(&mut self, ctx: &Context, a: Action) {
        match a {
            Action::Emoji if !self.has_modal() => {
                if let Some(mut selection) = self.editor_selection {
                    // Shortcuts run before this frame's editor layout. Read the
                    // live cursor state, which Undo/programmatic selection may have changed.
                    if let Some(range) = self.selection(ctx) {
                        selection.range = range;
                    }
                    self.selection_tools = Some(SelectionTools {
                        selection,
                        emoji: true,
                        query: String::new(),
                        focus_search: true,
                        rect: Rect::NOTHING,
                    });
                    self.seen_selection = Some(selection.key());
                }
            }
            Action::New => self.new_doc(),
            Action::Open => {
                let mut dlg = rfd::FileDialog::new()
                    .add_filter(tr(self.cfg.lang, "Files"), &["*"])
                    .add_filter("Markdown", &["md", "markdown", "txt"]);
                if let Some(d) = self.docs.get(self.active).and_then(Doc::dir) {
                    dlg = dlg.set_directory(d);
                }
                for p in dlg.pick_files().unwrap_or_default() {
                    self.open_path(p);
                }
            }
            Action::Save | Action::SaveAs if !self.docs.is_empty() => {
                self.save(self.active, a == Action::SaveAs);
            }
            Action::ExportPdf | Action::Print if !self.docs.is_empty() && !self.has_modal() => {
                let doc = &self.docs[self.active];
                let title = doc.title(self.cfg.lang);
                let path = if a == Action::ExportPdf {
                    let mut dialog = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name(format!("{}.pdf", doc.stem(self.cfg.lang)));
                    if let Some(dir) = doc.dir() {
                        dialog = dialog.set_directory(dir);
                    }
                    let Some(path) = dialog.save_file() else { return };
                    // Never replace the document itself, even if a PDF was opened as text.
                    if doc.path.as_ref().is_some_and(|source| {
                        source == &path
                            || source.canonicalize().ok().zip(path.canonicalize().ok()).is_some_and(|(a, b)| a == b)
                    }) {
                        self.error = Some(self.t("Choose a different file").into());
                        return;
                    }
                    Some(path)
                } else {
                    if self.print_job.is_some() {
                        return;
                    }
                    None
                };
                match self.document_pdf(ctx) {
                    Ok(bytes) => {
                        if let Some(path) = path {
                            if let Err(e) = std::fs::write(&path, bytes) {
                                self.error = Some(format!("{}: {e}", path.display()));
                            }
                        } else {
                            let (tx, rx) = std::sync::mpsc::channel();
                            self.print_job = Some(rx);
                            let ctx = ctx.clone();
                            std::thread::spawn(move || {
                                let _ = tx.send(crate::printing::print(&bytes, &title));
                                ctx.request_repaint();
                            });
                        }
                    }
                    Err(e) => self.error = Some(format!("{}: {e}", self.t("Export PDF…"))),
                }
            }
            Action::SaveAll => {
                for i in 0..self.docs.len() {
                    if self.docs[i].dirty() {
                        self.save(i, false);
                    }
                }
            }
            Action::Close if !self.docs.is_empty() => self.close(vec![self.docs[self.active].id]),
            Action::CloseAll => self.close(self.docs.iter().map(|d| d.id).collect()),
            Action::Copy => {
                if let (Some((a, b)), Some(doc)) = (self.selection(ctx), self.docs.get(self.active)) {
                    let s = &doc.text[char_to_byte(&doc.text, a)..char_to_byte(&doc.text, b)];
                    ctx.copy_text(s.to_owned());
                }
            }
            Action::Paste => {
                if !self.paste_image(ctx)
                    && let Some(t) = self.clipboard.as_mut().and_then(|c| c.get_text().ok())
                {
                    self.insert(ctx, &t.replace("\r\n", "\n"));
                }
            }
            Action::Find => {
                let f = self.find.get_or_insert_with(Find::default);
                f.focus = true;
                if let (Some((a, b)), Some(doc)) = (self.selection(ctx), self.docs.get(self.active))
                    && a != b
                {
                    let sel = doc.text[char_to_byte(&doc.text, a)..char_to_byte(&doc.text, b)].to_owned();
                    if !sel.contains('\n') {
                        self.find.as_mut().unwrap().query = sel;
                    }
                }
            }
            Action::NewPage => self.begin_new_page(ctx),
            Action::Insert => self.insert_image_file(ctx),
            Action::Preferences => self.settings = true,
            Action::About => self.about = true,
            Action::Undo => self.undo_redo(ctx, false),
            Action::Redo => self.undo_redo(ctx, true),
            Action::Files => self.cfg.files = !self.cfg.files,
            Action::CycleWidth => self.cfg.view_width = (self.cfg.view_width + 1) % 4,
            Action::Exit => self.exit(ctx),
            Action::PastePrimary => {
                if let Some(t) = self.primary_text() {
                    self.insert(ctx, &t.replace("\r\n", "\n"));
                }
            }
            Action::OpenRecent(i) => {
                if let Some(p) = self.recent.get(i).cloned() {
                    self.open_path(p);
                }
            }
            // Markdown formatting is only available for Markdown documents.
            Action::Fmt(f) if self.docs.get(self.active).is_some_and(|d| d.syntax.is_none()) => {
                self.apply_fmt(ctx, f);
            }
            _ => {}
        }
    }

    /// Detects Ctrl+V with an image-only clipboard: egui swallows the key and emits no Paste event then.
    fn detect_image_paste(&mut self, ctx: &Context) {
        let mut trigger = false;
        ctx.input(|i| {
            for e in &i.events {
                match e {
                    egui::Event::Paste(_) => self.pasted = true,
                    egui::Event::Key { key: Key::V, pressed: true, .. } => self.v_pressed = true,
                    egui::Event::Key { key: Key::V, pressed: false, modifiers, .. } => {
                        trigger = !self.v_pressed && !self.pasted && modifiers.command;
                        self.v_pressed = false;
                        self.pasted = false;
                    }
                    _ => {}
                }
            }
        });
        if trigger {
            self.paste_image(ctx);
        }
    }

    /// A separate print context keeps editor caches, textures, cursor and history untouched.
    fn document_pdf(&self, ctx: &Context) -> Result<Vec<u8>, String> {
        use crate::printing::{COLUMN, CONTENT_HEIGHT};
        let doc = &self.docs[self.active];
        let print_ctx = Context::default();
        print_ctx.set_fonts(ctx.fonts(|f| f.definitions().clone()));
        let mut look = self.cfg.look();
        look.size = 11.0;
        let dir = doc.dir();
        let mut images = HashMap::new();
        let mut page_breaks = vec![0.0];
        let output = print_ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(COLUMN, CONTENT_HEIGHT))),
                max_texture_side: Some(MAX_TEXTURE_SIDE as usize),
                ..Default::default()
            },
            |ui| {
                let ctx = ui.ctx();
                let mut image_size = |src: &str| {
                    let key = image_key(dir.as_deref(), src)?;
                    let image = images.entry(key.clone()).or_insert_with(|| load_image(ctx, dir.as_deref(), src));
                    let size = image.as_ref()?.1;
                    let scale = (COLUMN / size.x).min((CONTENT_HEIGHT - 20.0) / size.y).min(1.0);
                    Some((key, size * scale))
                };
                let (galley, decos) = if doc.syntax.is_none() {
                    md::build(&doc.text, None, COLUMN, &look, 1.0, &mut image_size, &mut |job| {
                        crate::emoji::layout(ctx.fonts_mut(|f| f.layout_job(job)))
                    })
                } else {
                    let mut job =
                        egui::text::LayoutJob::simple(doc.text.clone(), FontId::monospace(10.0), md::TEXT, COLUMN);
                    job.break_on_newline = true;
                    (crate::emoji::layout(ctx.fonts_mut(|f| f.layout_job(job))), vec![])
                };
                page_breaks = crate::printing::breaks(&galley);
                let painter = ctx
                    .layer_painter(egui::LayerId::new(egui::Order::Background, Id::new("print")))
                    .with_clip_rect(Rect::EVERYTHING);
                let mut backgrounds = vec![];
                // Reserve a slot for backgrounds, exactly as the live editor does.
                let background = painter.add(Shape::Noop);
                painter.galley(Pos2::ZERO, galley.clone(), md::TEXT);
                paint_decos(
                    &DecoCx {
                        ctx,
                        painter: &painter,
                        galley: &galley,
                        pos: Pos2::ZERO,
                        col_w: COLUMN,
                        look: &look,
                        images: &images,
                        pointer: None,
                        clicked: false,
                    },
                    &decos,
                    &mut backgrounds,
                );
                painter.set(background, Shape::Vec(backgrounds));
            },
        );
        crate::printing::pdf(&print_ctx, output, &page_breaks)
    }

    fn menu_bar(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let lang = self.cfg.lang;
        let t = |s| tr(lang, s);
        let sc = shortcuts();
        let label =
            |a: Action| sc.iter().find(|(_, _, x)| *x == a).map(|(m, k, _)| shortcut_label(*m, *k)).unwrap_or_default();
        let item = |ui: &mut Ui, acts: &mut Vec<Action>, text: &str, a: Action| {
            if ui.add(Button::new(text).shortcut_text(label(a))).clicked() {
                acts.push(a);
            }
        };
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button(t("File"), |ui| {
                item(ui, acts, t("New"), Action::New);
                item(ui, acts, t("Open…"), Action::Open);
                ui.menu_button(t("Recent"), |ui| {
                    if self.recent.is_empty() {
                        ui.label(RichText::new(t("(empty)")).color(md::DIM));
                    }
                    for (i, p) in self.recent.iter().enumerate() {
                        let name =
                            p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned());
                        let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
                        if ui.add(Button::new(name).shortcut_text(RichText::new(dir).small())).clicked() {
                            acts.push(Action::OpenRecent(i));
                        }
                    }
                });
                ui.separator();
                item(ui, acts, t("Save"), Action::Save);
                item(ui, acts, t("Save as…"), Action::SaveAs);
                item(ui, acts, t("Save all"), Action::SaveAll);
                ui.separator();
                ui.add_enabled_ui(!self.docs.is_empty() && !self.has_modal(), |ui| {
                    item(ui, acts, t("Export PDF…"), Action::ExportPdf);
                    ui.add_enabled_ui(self.print_job.is_none(), |ui| item(ui, acts, t("Print…"), Action::Print));
                });
                ui.separator();
                item(ui, acts, t("Close"), Action::Close);
                item(ui, acts, t("Close all"), Action::CloseAll);
                ui.separator();
                item(ui, acts, t("Exit"), Action::Exit);
            });
            ui.menu_button(t("Edit"), |ui| {
                item(ui, acts, t("Undo"), Action::Undo);
                if ui.add(Button::new(t("Redo")).shortcut_text("Ctrl+Y")).clicked() {
                    acts.push(Action::Redo);
                }
                ui.separator();
                if ui.add(Button::new(t("Copy")).shortcut_text("Ctrl+C")).clicked() {
                    acts.push(Action::Copy);
                }
                if ui.add(Button::new(t("Paste")).shortcut_text("Ctrl+V")).clicked() {
                    acts.push(Action::Paste);
                }
                ui.separator();
                item(ui, acts, t("Find/Replace"), Action::Find);
                item(ui, acts, t("Insert image…"), Action::Insert);
                item(ui, acts, t("Insert emoji…"), Action::Emoji);
                ui.separator();
                item(ui, acts, t("Preferences…"), Action::Preferences);
            });
            ui.menu_button(t("View"), |ui| {
                ui.menu_button(format!("{} ({})", t("View width"), label(Action::CycleWidth)), |ui| {
                    for (index, name) in VIEW_WIDTHS.iter().enumerate() {
                        if ui.selectable_value(&mut self.cfg.view_width, index as u8, t(name)).clicked() {
                            ui.close();
                        }
                    }
                });
                ui.checkbox(&mut self.cfg.line_numbers, t("Line numbers"));
                ui.checkbox(&mut self.cfg.inline_title, t("Inline title"));
                ui.checkbox(&mut self.cfg.files, t("Files")).on_hover_text(label(Action::Files));
            });
            ui.add_enabled_ui(!self.has_modal() && !self.docs.is_empty(), |ui| {
                ui.menu_button(t("Syntax"), |ui| {
                    let Some(doc) = self.docs.get_mut(self.active) else { return };
                    let mut mode = doc.syntax_mode;
                    let mut option = |ui: &mut Ui, value, name: &str| {
                        if ui.selectable_value(&mut mode, value, name).clicked() {
                            ui.close();
                        }
                    };
                    option(ui, SyntaxMode::Automatic, t("Automatic"));
                    ui.separator();
                    option(ui, SyntaxMode::Markdown, "Markdown");
                    for syntax in Syntax::available() {
                        option(ui, SyntaxMode::Language(syntax), syntax.name());
                    }
                    if mode != doc.syntax_mode {
                        doc.set_syntax(mode);
                    }
                });
            });
            if ui.button(t("About")).on_hover_text("F1").clicked() {
                acts.push(Action::About);
            }
        });
    }

    fn tabs(&mut self, ui: &mut Ui) {
        let lang = self.cfg.lang;
        let mut close = None;
        ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (i, d) in self.docs.iter().enumerate() {
                    let active = i == self.active;
                    let fill = if active { md::BG } else { CHROME };
                    egui::Frame::NONE
                        .fill(fill)
                        .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                        .inner_margin(Margin { left: 12, right: 6, top: 6, bottom: 6 })
                        .show(ui, |ui| {
                            let color = if active { md::HEAD } else { md::DIM.gamma_multiply(1.6) };
                            let name = if d.dirty() { format!("● {}", d.title(lang)) } else { d.title(lang) };
                            let l = ui.add(
                                Label::new(RichText::new(name).color(color)).sense(Sense::click()).selectable(false),
                            );
                            if l.clicked() {
                                self.active = i;
                            }
                            if l.middle_clicked() {
                                close = Some(d.id);
                            }
                            let x = Button::new(RichText::new("×").size(15.0).color(color)).frame(false);
                            if ui.add(x).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                close = Some(d.id);
                            }
                        });
                }
            });
        });
        if let Some(id) = close {
            self.close(vec![id]);
        }
    }

    /// Returns what the user clicked: a link (ctrl+click) or a text position (middle click).
    fn editor(&mut self, ui: &mut Ui) -> (Option<String>, Option<usize>) {
        let lang = self.cfg.lang;
        let ctx = ui.ctx().clone();
        self.editor_selection = None;
        let popup_selection = self.selection_tools.as_ref().filter(|tools| tools.emoji).map(|tools| tools.selection);
        let Some(doc) = self.docs.get_mut(self.active) else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(tr(lang, "Open a file with Ctrl+O or start typing in a new tab (Ctrl+N)."))
                        .color(md::DIM),
                );
            });
            return (None, None);
        };

        if let Some(p) = &doc.path {
            let r = ui.add(
                Label::new(RichText::new(p.display().to_string()).size(11.0).color(md::DIM))
                    .sense(Sense::click())
                    .selectable(false),
            );
            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                && let Some(d) = p.parent()
            {
                let _ = open::that(d);
            }
        }

        let mut find_act = None;
        if let Some(f) = &mut self.find {
            ui.horizontal(|ui| {
                let r = ui.add(TextEdit::singleline(&mut f.query).hint_text(tr(lang, "Find")).desired_width(220.0));
                if std::mem::take(&mut f.focus) {
                    r.request_focus();
                }
                if r.changed() {
                    f.none = false;
                }
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    find_act = Some(0);
                    r.request_focus();
                }
                ui.add(TextEdit::singleline(&mut f.replace).hint_text(tr(lang, "Replace")).desired_width(220.0));
                if ui.button(tr(lang, "Next")).clicked() {
                    find_act = Some(0);
                }
                if ui.button(tr(lang, "Replace")).clicked() {
                    find_act = Some(1);
                }
                if ui.button(tr(lang, "Replace all")).clicked() {
                    find_act = Some(2);
                }
                if f.none {
                    ui.label(RichText::new(tr(lang, "No matches")).color(md::DIM));
                }
                if ui.add(Button::new("×").frame(false)).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    find_act = Some(3);
                }
            });
            ui.add_space(4.0);
        }
        if let Some(a) = find_act {
            match a {
                0 => self.find_next(&ctx),
                1 => self.replace_one(&ctx),
                2 => self.replace_all(&ctx),
                _ => self.find = None,
            }
            return (None, None);
        }

        if self.docs.get(self.active).is_some_and(|d| highlight::Analysis::requires_worker(&d.text, d.syntax))
            && self.syntax_worker.is_none()
        {
            self.syntax_worker = Some(highlight::Worker::new(ctx.clone()));
        }
        let Self {
            docs, active, images, cfg, find, scroll_to, scroll_pending, syntax_worker, editor_selection, ..
        } = self;
        let doc = &mut docs[*active];
        let id = doc.edit_id();
        let dir = doc.dir();
        let syntax = doc.syntax;
        let gutter = if cfg.line_numbers { 48.0 } else { 0.0 };
        let full = ui.available_width();
        let (col_w, left) = view_geometry(full, gutter, cfg.view_width);
        let mut clicked_link = None;
        let mut middle = None;
        let now = self.now;
        let mut look = cfg.look();
        if syntax.is_some() {
            look.size += 1.5;
        }
        let font = cfg.font.clone();

        let mut area = ScrollArea::vertical().id_salt(("scroll", doc.id)).auto_shrink(false);
        if let Some(y) = scroll_pending.take() {
            area = area.vertical_scroll_offset(y);
        }
        let mut drag_scroll = 0.0;
        let out = area.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(left);
                ui.vertical(|ui| {
                    ui.set_width(col_w);
                    ui.add_space(32.0);
                    if cfg.inline_title {
                        ui.add(
                            Label::new(
                                RichText::new(doc.stem(lang))
                                    .size((look.size * TITLE_SCALE).round())
                                    .family(FontFamily::Name("b".into()))
                                    .color(md::HEAD),
                            )
                            .wrap(),
                        );
                        ui.add_space(12.0);
                    }
                    let bg = ui.painter().add(Shape::Noop);
                    let cursor_ci =
                        TextEditState::load(&ctx, id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0);
                    let ppp = ctx.pixels_per_point();
                    let cache = &mut doc.cache;
                    let analysis = &mut doc.analysis;
                    let code_layout = &mut doc.code_layout;
                    let mut font_key = std::collections::hash_map::DefaultHasher::new();
                    (&font, cfg.lang.code()).hash(&mut font_key);
                    code_layout.set_fonts(font_key.finish());
                    let document_id = doc.id;
                    let mut img_size = |src: &str| {
                        let key = image_key(dir.as_deref(), src)?;
                        let entry = images.entry(key.clone()).or_insert_with(|| load_image(&ctx, dir.as_deref(), src));
                        entry.as_ref().map(|(_, size)| (key, *size))
                    };
                    let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| {
                        let text = buf.as_str();
                        analysis.update(document_id, text, syntax, syntax_worker.as_ref());
                        let line = syntax
                            .is_none()
                            .then(|| cursor_ci.map(|c| text.chars().take(c).filter(|ch| *ch == '\n').count()))
                            .flatten();
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        let l = &look;
                        (
                            analysis.revision(),
                            line,
                            wrap.to_bits(),
                            ppp.to_bits(),
                            l.size.to_bits(),
                            l.line_height.to_bits(),
                        )
                            .hash(&mut h);
                        (l.weight, l.bold_weight, l.code_weight, &font, syntax).hash(&mut h);
                        let key = h.finish();
                        if let Some(c) = cache.as_ref().filter(|c| c.key == key) {
                            return c.galley.clone();
                        }
                        let mut layout = |job| crate::emoji::layout(ui.ctx().fonts_mut(|f| f.layout_job(job)));
                        let (galley, decos) = match syntax {
                            Some(_) => (
                                highlight::build_spans(
                                    text,
                                    analysis.spans(),
                                    wrap,
                                    &look,
                                    ppp,
                                    &mut layout,
                                    code_layout,
                                ),
                                vec![],
                            ),
                            None => md::build_colored(
                                text,
                                cursor_ci,
                                wrap,
                                &look,
                                ppp,
                                analysis.spans(),
                                &mut img_size,
                                &mut layout,
                            ),
                        };
                        *cache = Some(Cache { key, line, galley: galley.clone(), decos });
                        galley
                    };
                    let min_h = (ui.available_height() - 20.0).max(200.0);
                    // Link navigation must not move the source cursor and reveal a rendered block.
                    let navigating = ctx.input(|i| {
                        (i.modifiers.command || i.modifiers.ctrl)
                            && (i.pointer.primary_down() || i.pointer.primary_clicked() || i.pointer.primary_released())
                    });
                    let out = TextEdit::multiline(&mut doc.text)
                        .interactive(!navigating)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(Margin::ZERO)
                        .desired_width(col_w)
                        .min_size(vec2(col_w, min_h))
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
                    let endpoints = popup_selection
                        .filter(|selection| selection.doc_id == doc.id)
                        .map(|selection| selection.range)
                        .or_else(|| {
                            out.cursor_range.map(|range| {
                                (
                                    range.primary.index.0.min(range.secondary.index.0),
                                    range.primary.index.0.max(range.secondary.index.0),
                                )
                            })
                        });
                    if let Some((a, b)) = endpoints {
                        *editor_selection = Some(EditorSelection {
                            doc_id: doc.id,
                            range: (a, b),
                            anchor: out.galley.pos_from_cursor(CCursor::new(b)).translate(out.galley_pos.to_vec2()).min,
                        });
                    }
                    // While a drag-selection runs, egui ignores the wheel and does not scroll toward a
                    // pointer that left the view, so both are done here (see `scroll_pending` below).
                    if ctx.dragged_id() == Some(id) {
                        let view = ui.clip_rect();
                        let (wheel, pointer) = ctx.input(|i| (i.smooth_scroll_delta.y, i.pointer.latest_pos()));
                        let edge = pointer.map_or(0.0, |p| {
                            if p.y < view.top() {
                                p.y - view.top()
                            } else if p.y > view.bottom() {
                                p.y - view.bottom()
                            } else {
                                0.0
                            }
                        });
                        // Positive offsets scroll down, the wheel delta is the other way round.
                        drag_scroll = edge * 0.5 - wheel;
                    }
                    ui.add_space(40.0);

                    let Some(cache) = &doc.cache else { return };
                    // Re-layout next frame when the cursor moved to another line (to reveal its markers).
                    let new_line = if syntax.is_none() {
                        out.cursor_range
                            .map(|r| doc.text.chars().take(r.primary.index.0).filter(|ch| *ch == '\n').count())
                    } else {
                        None
                    };
                    if new_line.is_some() && new_line != cache.line {
                        ctx.request_repaint();
                    }

                    let g = &out.galley;
                    let o = out.galley_pos.to_vec2();
                    let gx = out.galley_pos.x;
                    let at = |ci: usize| g.pos_from_cursor(CCursor::new(ci)).translate(o);
                    let painter = ui.painter();
                    let pointer = ctx.input(|i| i.pointer.interact_pos());
                    let clicked = out.response.clicked() || (navigating && ctx.input(|i| i.pointer.primary_clicked()));
                    let mut bgs = vec![];
                    let deco_cx = DecoCx {
                        ctx: &ctx,
                        painter,
                        galley: g,
                        pos: out.galley_pos,
                        col_w,
                        look: &look,
                        images,
                        pointer,
                        clicked,
                    };
                    let events = paint_decos(&deco_cx, &cache.decos, &mut bgs);
                    let toggle = events.toggle;
                    if events.link.is_some() {
                        clicked_link = events.link;
                    }
                    if let Some((a, b)) = find.as_ref().and_then(|f| f.hl) {
                        let (r1, r2) = (at(a), at(b));
                        if (r1.min.y - r2.min.y).abs() < 1.0 {
                            let r = Rect::from_min_max(r1.min, pos2(r2.max.x, r1.max.y));
                            bgs.push(Shape::rect_filled(r, 2.0, md::ACCENT.gamma_multiply(0.3)));
                        }
                    }
                    ui.painter().set(bg, Shape::Vec(bgs));

                    if cfg.line_numbers {
                        let mut num = 1;
                        let mut starts = true;
                        for row in &g.rows {
                            if starts {
                                let r = row.rect().translate(o);
                                if r.height() >= 10.0 {
                                    painter.text(
                                        pos2(gx - 14.0, r.center().y),
                                        Align2::RIGHT_CENTER,
                                        num.to_string(),
                                        FontId::monospace(12.0),
                                        md::DIM,
                                    );
                                }
                                num += 1;
                            }
                            starts = row.ends_with_newline;
                        }
                    }
                    if let Some(ci) = scroll_to.take() {
                        ui.scroll_to_rect(at(ci).expand(60.0), Some(Align::Center));
                    }
                    if let Some((byte, checked)) = toggle {
                        doc.checkpoint(now);
                        doc.text.replace_range(byte..byte + 1, if checked { " " } else { "x" });
                        doc.checkpoint(now);
                    }
                    if out.response.middle_clicked()
                        && let Some(p) = pointer
                    {
                        middle = Some(g.cursor_from_pos(p - out.galley_pos).index.0);
                    }
                });
            });
        });
        if drag_scroll != 0.0 {
            // Offsets set by `scroll_with_delta` are not applied reliably while a drag is active,
            // so the new offset is handed to the next frame's ScrollArea instead.
            *scroll_pending = Some((out.state.offset.y + drag_scroll).max(0.0));
            ctx.request_repaint();
        }
        (clicked_link, middle)
    }

    fn modals(&mut self, ctx: &Context) {
        // Enter presses the dialog's default button.
        let enter = ctx.input(|i| i.key_pressed(Key::Enter));
        let lang = self.cfg.lang;
        let t = |s| tr(lang, s);
        if let Some((id, external)) = self.file_prompt {
            if let Some(i) = self.docs.iter().position(|d| d.id == id) {
                self.active = i;
                let name = self.docs[i].title(lang);
                let mut choice = None;
                dialog(
                    ctx,
                    "file_change_modal",
                    t(if external { "File changed on disk" } else { "Unsaved changes" }),
                    460.0,
                    |ui| {
                        ui.label(&name);
                        ui.label(t(if external {
                            "The file was changed outside the editor. What would you like to do?"
                        } else {
                            "Save changes before opening this file again?"
                        }));
                        ui.add_space(16.0);
                        centered_row(ui, |ui| {
                            if external && ui.button(t("Reload")).clicked() {
                                choice = Some(0);
                            }
                            if ui.button(t("Save")).clicked() {
                                choice = Some(1);
                            }
                            if ui.button(t("Do nothing")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                                choice = Some(2);
                            }
                        });
                    },
                );
                match choice {
                    Some(0) if self.reload_doc(i) => self.file_prompt = None,
                    Some(1) if self.save(i, false) => self.file_prompt = None,
                    Some(2) => self.file_prompt = None,
                    _ => {}
                }
            } else {
                self.file_prompt = None;
            }
        }
        if let Some(&id) = self.close_queue.first() {
            let Some(i) = self.docs.iter().position(|d| d.id == id) else {
                self.close_queue.remove(0);
                return;
            };
            self.active = i;
            let name = self.docs[i].title(lang);
            let mut choice = None;
            dialog(ctx, "close_modal", t("Unsaved changes"), 380.0, |ui| {
                ui.label(format!("{} „{name}”?", t("Save changes to")));
                ui.add_space(16.0);
                centered_row(ui, |ui| {
                    if ui.button(t("Save")).clicked() || enter {
                        choice = Some(0);
                    }
                    if ui.button(t("Don't save")).clicked() {
                        choice = Some(1);
                    }
                    if ui.button(t("Cancel")).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) if self.save(i, false) => {
                    self.close_queue.remove(0);
                    self.remove_doc(i);
                }
                Some(1) => {
                    self.close_queue.remove(0);
                    self.remove_doc(i);
                }
                Some(2) => {
                    self.close_queue.clear();
                    self.quit_after = false;
                }
                _ => {}
            }
            if self.close_queue.is_empty() && self.quit_after {
                self.allow_quit = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }

        if let Some(e) = &self.error {
            let r = dialog(ctx, "error_modal", t("Error"), 380.0, |ui| {
                ui.label(e);
                ui.add_space(16.0);
                centered_row(ui, |ui| ui.button("OK").clicked() || enter)
            });
            if r.inner || r.should_close() {
                self.error = None;
            }
        }

        if let Some(page) = &mut self.new_page {
            let mut choice = None;
            let r = dialog(ctx, "new_page_modal", t("New linked page"), 420.0, |ui| {
                ui.label(t("File name:"));
                let response =
                    ui.add(TextEdit::singleline(&mut page.name).hint_text("note.md").desired_width(f32::INFINITY));
                if page.focus {
                    response.request_focus();
                    page.focus = false;
                }
                if let Some(error) = &page.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                ui.add_space(16.0);
                centered_row(ui, |ui| {
                    if ui.button(t("Save")).clicked() || enter {
                        choice = Some(true);
                    }
                    if ui.button(t("Cancel")).clicked() {
                        choice = Some(false);
                    }
                });
            });
            if r.should_close() || ctx.input(|i| i.key_pressed(Key::Escape)) {
                choice = Some(false);
            }
            match choice {
                Some(true) => {
                    if let Err(error) = self.create_linked_page(ctx)
                        && let Some(page) = &mut self.new_page
                    {
                        page.error = Some(error);
                    }
                }
                Some(false) => {
                    self.new_page = None;
                    if let Some((a, b)) = self.selection(ctx) {
                        self.set_selection(ctx, a, b);
                    }
                }
                None => {}
            }
        }

        if let Some(name) = &mut self.new_file {
            let mut choice = None;
            let r = dialog(ctx, "new_file_modal", t("Create new"), 380.0, |ui| {
                ui.label(t("File name:"));
                ui.add(TextEdit::singleline(name).hint_text("note.md").desired_width(f32::INFINITY)).request_focus();
                ui.add_space(16.0);
                centered_row(ui, |ui| {
                    if ui.button(t("Save")).clicked() || enter {
                        choice = Some(true);
                    }
                    if ui.button(t("Cancel")).clicked() {
                        choice = Some(false);
                    }
                });
            });
            if r.should_close() {
                choice = Some(false);
            }
            match choice {
                Some(true) => self.create_file(),
                Some(false) => self.new_file = None,
                None => {}
            }
        }

        if let Some(path) = self.delete_file.clone() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let mut choice = None;
            let r = dialog(ctx, "delete_modal", t("Delete file"), 400.0, |ui| {
                ui.label(format!("{} „{name}”?", t("Delete")));
                ui.add_space(16.0);
                centered_row(ui, |ui| {
                    if ui.button(t("Move to trash")).clicked() || enter {
                        choice = Some(Some(true));
                    }
                    if ui.button(t("Delete permanently")).clicked() {
                        choice = Some(Some(false));
                    }
                    if ui.button(t("Cancel")).clicked() {
                        choice = Some(None);
                    }
                });
            });
            if r.should_close() {
                choice = Some(None);
            }
            if let Some(how) = choice {
                self.delete_file = None;
                if let Some(trash) = how {
                    self.delete(&path, trash);
                }
            }
        }

        if self.settings {
            let r = dialog(ctx, "settings_modal", t("Preferences"), 440.0, |ui| self.settings_ui(ui));
            if r.inner || r.should_close() {
                self.settings = false;
            }
        }

        if self.about {
            let r = dialog(ctx, "about_modal", &format!("{} {}", t("About"), crate::APP_TITLE), 460.0, |ui| {
                ui.label(RichText::new(t("Keyboard shortcuts")).strong().color(md::HEAD));
                ui.add_space(6.0);
                let max_h = (ctx.content_rect().height() * 0.55).max(200.0);
                egui::ScrollArea::vertical()
                    .max_height(max_h)
                    .auto_shrink([false, true])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        egui::Grid::new("shortcuts").num_columns(3).spacing([20.0, 5.0]).show(ui, |ui| {
                            for row in about_rows(lang) {
                                ui.label(RichText::new(row.what).color(md::MUTED));
                                ui.label(RichText::new(row.keys).color(md::LINK));
                                ui.label(RichText::new(row.syntax).monospace().color(md::CODE));
                                ui.end_row();
                            }
                        });
                    });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.label("ORDIGITAL Adam Mateusz Brożyński");
                    ui.label(format!("{}: MIT", t("License")));
                    ui.label(format!("{} Claude/Codex.", t("Code written with the help of")));
                });
                ui.add_space(16.0);
                let close = centered_row(ui, |ui| ui.button("OK").clicked() || enter);
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    ui.hyperlink_to(COFFEE_LABEL, COFFEE_URL);
                });
                close
            });
            if r.inner || r.should_close() {
                self.about = false;
            }
        }
    }

    /// Settings dialog body; returns true when it should close.
    fn settings_ui(&mut self, ui: &mut Ui) -> bool {
        let lang = self.cfg.lang;
        let t = |s| tr(lang, s);
        if self.font_list.is_empty() {
            self.font_list = fonts::families();
        }
        egui::Grid::new("settings").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
            ui.label(t("Editor font"));
            let current = if self.cfg.font.is_empty() { fonts::BUILTIN } else { self.cfg.font.as_str() };
            // Windows has no fontconfig to list fonts from: only the built-in font is offered.
            if cfg!(windows) {
                self.cfg.font.clear();
                ui.label(fonts::BUILTIN);
            } else {
                egui::ComboBox::from_id_salt("font").selected_text(current).width(240.0).height(360.0).show_ui(
                    ui,
                    |ui| {
                        ui.add(TextEdit::singleline(&mut self.font_filter).hint_text(t("Filter")).desired_width(220.0))
                            .request_focus();
                        let q = self.font_filter.to_lowercase();
                        if q.is_empty() || fonts::BUILTIN.to_lowercase().contains(&q) {
                            ui.selectable_value(&mut self.cfg.font, String::new(), fonts::BUILTIN);
                        }
                        for f in self.font_list.iter().filter(|f| f.to_lowercase().contains(&q)) {
                            ui.selectable_value(&mut self.cfg.font, f.clone(), f);
                        }
                    },
                );
            }
            ui.end_row();

            ui.label(t("Text weight"));
            ui.add(egui::Slider::new(&mut self.cfg.weight, 100..=900).step_by(100.0));
            ui.end_row();

            ui.label(t("Bold weight"));
            ui.add(egui::Slider::new(&mut self.cfg.bold_weight, 100..=900).step_by(100.0));
            ui.end_row();

            ui.label(t("Code weight"));
            ui.add(egui::Slider::new(&mut self.cfg.code_weight, 200..=900).step_by(100.0));
            ui.end_row();

            ui.label(t("Text gamma"));
            ui.add(egui::Slider::new(&mut self.cfg.gamma, 0.4..=1.4).step_by(0.05))
                .on_hover_text(t("Lower values make text bolder and brighter"));
            ui.end_row();

            ui.label(t("Font size"));
            ui.add(egui::Slider::new(&mut self.cfg.size, 8.0..=48.0).step_by(0.5).suffix(" px"));
            ui.end_row();

            ui.label(t("Line height"));
            ui.add(egui::Slider::new(&mut self.cfg.line_height, 1.0..=2.5).step_by(0.05));
            ui.end_row();

            ui.label(t("Pasted images"));
            ui.checkbox(&mut self.cfg.embed_images, t("Embed in the document (JPEG)"))
                .on_hover_text(t("Otherwise saved as WebP files next to the document"));
            ui.end_row();

            ui.label(t("Language"));
            egui::ComboBox::from_id_salt("language")
                .selected_text(self.cfg.lang.label())
                .width(240.0)
                .height(360.0)
                .show_ui(ui, |ui| {
                    for lang in Lang::ALL {
                        ui.selectable_value(&mut self.cfg.lang, lang, lang.label());
                    }
                });
            ui.end_row();
        });
        ui.add_space(16.0);
        centered_row(ui, |ui| {
            if ui.button(t("Reset")).on_hover_text(t("Restore default font settings")).clicked() {
                self.cfg.reset_text();
            }
            // Enter also closes, unless a text field (e.g. the font filter) is being typed in.
            let enter = ui.input(|i| i.key_pressed(Key::Enter)) && !ui.ctx().egui_wants_keyboard_input();
            ui.button("OK").clicked() || enter
        })
    }

    fn status_bar(&self, ui: &mut Ui) {
        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
            ui.hyperlink_to(RichText::new(COFFEE_LABEL).small(), COFFEE_URL);
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| self.document_status(ui));
        });
    }

    fn document_status(&self, ui: &mut Ui) {
        let Some(doc) = self.docs.get(self.active) else {
            return;
        };
        let t = |s| tr(self.cfg.lang, s);
        let (chars, words) = doc.stats();
        let links = doc.cache.as_ref().map_or(0, |c| c.decos.iter().filter(|d| matches!(d, Deco::Link { .. })).count());
        let kb = doc.text.len() as f32 / 1024.0;
        let kb = if self.cfg.lang.decimal_comma() { format!("{kb:.1}").replace('.', ",") } else { format!("{kb:.1}") };
        let s = format!(
            "{}: {chars}   ·   {}: {words}   ·   {}: {links}   ·   {kb} KB",
            t("characters"),
            t("words"),
            t("links")
        );
        ui.label(RichText::new(s).small().color(md::MUTED));
    }
}

/// What painting the decorations found out about the pointer.
#[derive(Default)]
struct DecoEvents {
    /// A checkbox that was clicked: (byte of its `[ ]`/`[x]` mark, was checked).
    toggle: Option<(usize, bool)>,
    /// A link that was ctrl+clicked.
    link: Option<String>,
}

/// Everything needed to paint the editor's decorations over its galley.
struct DecoCx<'a> {
    ctx: &'a Context,
    painter: &'a egui::Painter,
    galley: &'a Galley,
    /// Where the galley was painted.
    pos: Pos2,
    col_w: f32,
    look: &'a md::Look,
    images: &'a HashMap<String, Option<(TextureHandle, Vec2)>>,
    pointer: Option<Pos2>,
    clicked: bool,
}

/// Paints bullets, checkboxes, quote bars, rules, tables, formulas, images…; backgrounds go to `bgs`
/// (they are inserted behind the text by the caller).
fn paint_decos(cx: &DecoCx, decos: &[Deco], bgs: &mut Vec<Shape>) -> DecoEvents {
    let at = |ci: usize| cx.galley.pos_from_cursor(CCursor::new(ci)).translate(cx.pos.to_vec2());
    let mut events = DecoEvents::default();
    crate::emoji::paint(cx.ctx, cx.painter, cx.galley, cx.pos);
    for d in decos {
        match d {
            Deco::Code(a, b) => {
                let r = Rect::from_min_max(
                    pos2(cx.pos.x - 10.0, at(*a).min.y - 2.0),
                    pos2(cx.pos.x + cx.col_w + 10.0, at(*b).max.y + 2.0),
                );
                bgs.push(Shape::rect_filled(r, 4.0, md::CODE_BG));
            }
            Deco::Quote { start, end, level, indent, color } => {
                let (r1, r2) = (at(*start), at(*end));
                for k in 0..*level {
                    let x = cx.pos.x + (indent + k) as f32 * cx.look.indent() + 1.0;
                    let r = Rect::from_min_max(pos2(x, r1.min.y), pos2(x + 2.0, r2.max.y));
                    cx.painter.rect_filled(r, 0.0, color.unwrap_or(md::ACCENT));
                }
                if let Some(c) = color {
                    let left = cx.pos.x + *indent as f32 * cx.look.indent();
                    let r = Rect::from_min_max(pos2(left, r1.min.y), pos2(cx.pos.x + cx.col_w, r2.max.y));
                    bgs.push(Shape::rect_filled(r, 0.0, c.gamma_multiply(0.1)));
                }
            }
            Deco::Math { ci, galley, boxed, display, shift } => {
                let r = at(*ci);
                let size = galley.size();
                let x = if *display {
                    cx.pos.x + (cx.col_w - size.x) / 2.0
                } else {
                    r.min.x - shift + if *boxed { 6.0 } else { 1.0 }
                };
                let pos = pos2(x, r.center().y - size.y / 2.0);
                cx.painter.galley(pos, galley.clone(), md::TEXT);
                crate::emoji::paint(cx.ctx, cx.painter, galley, pos);
                if *boxed {
                    let pad = if *display { vec2(12.0, 8.0) } else { vec2(4.0, 1.0) };
                    let frame = Rect::from_min_size(pos, size).expand2(pad);
                    cx.painter.rect_stroke(frame, 4.0, Stroke::new(1.0, md::MUTED), egui::StrokeKind::Outside);
                }
            }
            Deco::Hr(ci) => {
                let y = at(*ci).center().y;
                cx.painter.hline(cx.pos.x..=cx.pos.x + cx.col_w, y, Stroke::new(2.0, md::BORDER));
            }
            Deco::Bullet(ci) => {
                let r = at(*ci);
                let x = r.min.x - cx.look.indent() * 0.5;
                cx.painter.circle_filled(pos2(x, r.center().y), (cx.look.size * 0.15).max(2.5), md::DIM);
            }
            Deco::Table { rows, widths, align } => {
                if let Some(url) = paint_table(cx, rows, widths, align, &at) {
                    events.link = Some(url);
                }
            }
            Deco::Check { ci, byte, checked } => {
                let r = at(*ci);
                let side = (cx.look.size * 1.0).round();
                let center = pos2(r.min.x - cx.look.indent() * 0.55, r.center().y);
                let bx = Rect::from_center_size(center, vec2(side, side));
                if *checked {
                    cx.painter.rect_filled(bx, 4.0, md::ACCENT);
                    let pts = [
                        bx.lerp_inside(vec2(0.22, 0.52)),
                        bx.lerp_inside(vec2(0.42, 0.72)),
                        bx.lerp_inside(vec2(0.78, 0.3)),
                    ];
                    cx.painter.line(pts.to_vec(), Stroke::new(2.0, md::BG));
                } else {
                    cx.painter.rect_stroke(bx, 4.0, Stroke::new(1.5, md::DIM), egui::StrokeKind::Inside);
                }
                if cx.pointer.is_some_and(|p| bx.expand(3.0).contains(p)) {
                    cx.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                    if cx.clicked {
                        events.toggle = Some((*byte, *checked));
                    }
                }
            }
            Deco::Image { ci, key, size } => {
                let r = at(*ci);
                let tex = cx.images.get(key).and_then(|t| t.as_ref());
                if let Some((tex, _)) = tex {
                    let rect = Rect::from_min_size(pos2(r.min.x, r.min.y + 6.0), *size);
                    cx.painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
                }
            }
            Deco::Link { start, end, url } => {
                if link_hit(cx, cx.galley, cx.pos, *start, *end) && cx.clicked {
                    events.link = Some(url.clone());
                }
            }
        }
    }
    events
}

fn link_hit(cx: &DecoCx, galley: &Galley, pos: Pos2, start: usize, end: usize) -> bool {
    if !cx.ctx.input(|i| i.modifiers.command || i.modifiers.ctrl) {
        return false;
    }
    let Some(pointer) = cx.pointer.filter(|p| cx.painter.clip_rect().contains(*p)) else { return false };
    let local = pointer - pos;
    // Check glyph rows, not nearest-cursor alone (which also matches blank cell space).
    let hit = galley.rows.iter().any(|row| {
        let rect = row.rect().translate(pos.to_vec2());
        rect.contains(pointer)
    }) && (start..end).contains(&galley.cursor_from_pos(local).index.0);
    if hit {
        cx.ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    hit
}

/// Draws a markdown table over the hidden source lines (each line reserved its row's height).
fn paint_table(
    cx: &DecoCx,
    rows: &[md::TableRow],
    widths: &[f32],
    align: &[Align],
    at: &dyn Fn(usize) -> Rect,
) -> Option<String> {
    let painter = cx.painter;
    let x0 = cx.pos.x;
    let mut clicked = None;
    const PAD: f32 = 12.0;
    let right = x0 + widths.iter().sum::<f32>();
    let stroke = Stroke::new(1.0, md::BORDER);
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return None;
    };
    let (top, bottom) = (at(first.ci).min.y, at(last.ci).max.y);
    for (r, row) in rows.iter().enumerate() {
        let line = at(row.ci);
        let (y0, y1) = (if r == 0 { top } else { line.min.y }, line.max.y);
        if r == 0 {
            painter.rect_filled(Rect::from_min_max(pos2(x0, y0), pos2(right, y1)), 0.0, md::CODE_BG);
        }
        painter.hline(x0..=right, y1, stroke);
        let mut x = x0;
        for (c, w) in widths.iter().enumerate() {
            if let Some(g) = row.cells.get(c) {
                let cell = Rect::from_min_max(pos2(x + PAD, y0), pos2(x + w - PAD, y1));
                let gx = match align.get(c) {
                    Some(Align::Center) => cell.center().x - g.size().x / 2.0,
                    Some(Align::Max) => cell.max.x - g.size().x,
                    _ => cell.min.x,
                };
                let pos = pos2(gx, cell.center().y - g.size().y / 2.0);
                painter.galley(pos, g.clone(), md::TEXT);
                crate::emoji::paint(cx.ctx, painter, g, pos);
                for link in &row.links[c] {
                    if let Deco::Link { start, end, url } = link
                        && link_hit(cx, g, pos, *start, *end)
                        && cx.clicked
                    {
                        clicked = Some(url.clone());
                    }
                }
            }
            x += w;
        }
    }
    painter.hline(x0..=right, top, stroke);
    let mut x = x0;
    painter.vline(x, top..=bottom, stroke);
    for w in widths {
        x += w;
        painter.vline(x, top..=bottom, stroke);
    }
    clicked
}

/// Lays out `add` in a horizontal row centered in the available width
/// (the row's width is measured on the previous frame).
fn centered_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let id = ui.id().with("centered_row");
    let w: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
        let r = ui.horizontal(add);
        let nw = r.response.rect.width();
        if (nw - w).abs() > 0.5 {
            ui.data_mut(|d| d.insert_temp(id, nw));
            ui.ctx().request_repaint();
        }
        r.inner
    })
    .inner
}

/// A modal with a shaded title bar and roomy margins.
fn dialog<R>(
    ctx: &Context,
    id: &str,
    title: &str,
    width: f32,
    body: impl FnOnce(&mut Ui) -> R,
) -> egui::ModalResponse<R> {
    let id = Id::new(id);
    let width_id = id.with("width");
    // Grows to fit content that turned out wider than `width` (measured on the previous frame).
    let width = ctx.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(0.0).max(width);
    let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(0).fill(CHROME);
    egui::Modal::new(id).frame(frame).show(ctx, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::Frame::NONE
            .fill(Color32::from_rgb(0x36, 0x32, 0x45))
            .corner_radius(CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
            .inner_margin(Margin::symmetric(20, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(Label::new(RichText::new(title).strong().color(md::HEAD)).selectable(false));
            });
        let inner =
            egui::Frame::NONE.inner_margin(Margin { left: 24, right: 24, top: 18, bottom: 20 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                body(ui)
            });
        let used = inner.response.rect.width();
        if used > width + 0.5 {
            ctx.data_mut(|d| d.insert_temp(width_id, used));
            ctx.request_repaint();
        }
        inner.inner
    })
}

/// Moves a file to the desktop trash using the desktop's own tools (`gio`, or KDE's `kioclient`).
#[cfg(windows)]
fn move_to_trash(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // The path goes through an environment variable, so it needs no quoting.
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($env:MDEDIT_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')";
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("MDEDIT_PATH", path)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()) }
}

#[cfg(not(windows))]
fn move_to_trash(path: &Path) -> Result<(), String> {
    let tools: [(&str, &[&str]); 3] = [("gio", &["trash"]), ("kioclient6", &["move"]), ("kioclient5", &["move"])];
    for (tool, args) in tools {
        let mut cmd = std::process::Command::new(tool);
        cmd.args(args).arg(path);
        if tool != "gio" {
            cmd.arg("trash:/");
        }
        if cmd.output().is_ok_and(|o| o.status.success()) {
            return Ok(());
        }
    }
    Err("no trash tool (gio / kioclient) available".into())
}

/// Parses the start of a list or quote line: (prefix to repeat — indentation and `> `,
/// the marker for the next item, byte where the item text starts).
fn list_prefix(line: &str) -> Option<(String, String, usize)> {
    let p = markdown::prefix(line);
    let prefix = line[..p.marker_start].to_owned();
    let next = match p.marker {
        Some(ListMarker::Task(_)) => "- [ ] ".to_owned(),
        Some(ListMarker::Bullet(marker)) if p.content_start > p.marker_start + 1 => format!("{} ", marker as char),
        Some(ListMarker::Ordered { digits, delimiter }) => {
            let n: u64 = line[p.marker_start..p.marker_start + digits].parse().ok()?;
            format!("{}{} ", n.saturating_add(1), delimiter as char)
        }
        _ => return (p.quotes > 0).then(|| (line[..p.quote_end].to_owned(), String::new(), p.quote_end)),
    };
    Some((prefix, next, p.content_start))
}

/// Byte replacements for sibling numbering; child lists keep independent counters.
fn numbered_edits(text: &str) -> Vec<(usize, usize, String)> {
    let mut edits = vec![];
    let mut stack: Vec<(String, char, u64)> = vec![];
    let mut offset = 0;
    let mut fence: Option<Fence> = None;
    let front_end = markdown::front_matter_end(text);
    for (li, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches(['\n', '\r']);
        let t = line.trim_start();
        if front_end.is_some_and(|end| li <= end) {
            offset += raw.len();
            continue;
        }
        if let Some(open) = fence {
            if open.closes(t) {
                fence = None;
            }
            offset += raw.len();
            continue;
        }
        if let Some(open) = Fence::opening(t) {
            fence = Some(open);
            stack.clear();
        } else if let parsed = markdown::prefix(line)
            && (parsed.marker.is_some() || parsed.quotes > 0)
        {
            if parsed.marker.is_some() {
                let prefix = &line[..parsed.marker_start];
                // Match visual indentation, including mixed tabs/spaces produced by Tab.
                let key = markdown::prefix_key(prefix);
                while stack.last().is_some_and(|(p, _, _)| p != &key && !key.starts_with(p.as_str())) {
                    stack.pop();
                }
                if let Some(ListMarker::Ordered { digits, delimiter }) = parsed.marker {
                    let delim = delimiter as char;
                    let Ok(number) = line[parsed.marker_start..parsed.marker_start + digits].parse::<u64>() else {
                        offset += raw.len();
                        continue;
                    };
                    let expected = match stack.last_mut() {
                        Some((p, d, n)) if *p == key && *d == delim => {
                            *n = n.saturating_add(1);
                            *n
                        }
                        _ => {
                            // A new nested list starts at one, including after deleting its first item.
                            let start = if key.starts_with(' ') || key.contains("  ") { 1 } else { number };
                            stack.push((key.clone(), delim, start));
                            start
                        }
                    };
                    if number != expected {
                        edits.push((offset + prefix.len(), offset + prefix.len() + digits, expected.to_string()));
                    }
                } else {
                    stack.retain(|(p, _, _)| p != &key);
                }
            } else {
                stack.clear();
            }
        } else if !t.is_empty() && !line.starts_with([' ', '\t']) {
            stack.clear();
        }
        offset += raw.len();
    }
    edits
}

/// Toggles a line-level markdown prefix on a single line.
fn toggle_line(l: &str, f: Fmt) -> String {
    let indent_len = l.len() - l.trim_start().len();
    let (indent, body) = l.split_at(indent_len);
    let parsed = markdown::prefix(body);
    let task = match parsed.marker {
        Some(ListMarker::Task(checked)) if parsed.quotes == 0 => Some((checked, &body[parsed.content_start..])),
        _ => None,
    };
    match f {
        Fmt::Heading(n) => {
            let hashes = body.bytes().take_while(|b| *b == b'#').count();
            let level = if hashes > 0 && body[hashes..].starts_with(' ') { hashes } else { 0 };
            let rest = if level > 0 { &body[level + 1..] } else { body };
            if n == 0 || level == n as usize { rest.to_owned() } else { format!("{} {rest}", "#".repeat(n as usize)) }
        }
        Fmt::Bullet => match task {
            Some((_, r)) => format!("{indent}- {r}"),
            None if parsed.quotes == 0 && matches!(parsed.marker, Some(ListMarker::Bullet(_))) => {
                format!("{indent}{}", &body[parsed.content_start..])
            }
            None => format!("{indent}- {body}"),
        },
        Fmt::Ordered => {
            if parsed.quotes == 0 && matches!(parsed.marker, Some(ListMarker::Ordered { .. })) {
                format!("{indent}{}", &body[parsed.content_start..])
            } else {
                format!("{indent}1. {body}")
            }
        }
        Fmt::Task => match task {
            Some((false, r)) => format!("{indent}- [x] {r}"),
            Some((true, r)) => format!("{indent}{r}"),
            None => {
                let rest = if parsed.quotes == 0 && matches!(parsed.marker, Some(ListMarker::Bullet(_))) {
                    &body[parsed.content_start..]
                } else {
                    body
                };
                format!("{indent}- [ ] {rest}")
            }
        },
        Fmt::Quote => match body.strip_prefix("> ") {
            Some(r) => format!("{indent}{r}"),
            None => format!("{indent}> {body}"),
        },
        _ => l.to_owned(),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);
        if let Some(job) = &self.print_job
            && let Ok(result) = job.try_recv()
        {
            self.print_job = None;
            if let Err(e) = result {
                self.error = Some(format!("{}: {e}", self.t("Print…")));
            }
        }
        if let Some(worker) = &self.syntax_worker {
            for output in worker.drain() {
                if let Some(doc) = self.docs.iter_mut().find(|doc| doc.id == output.id) {
                    doc.analysis.accept(output);
                }
            }
        }

        for file in ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files)) {
            if file.path().is_file() {
                self.open_path(file.path().to_owned());
            }
        }

        while let Ok(files) = self.rx.try_recv() {
            for f in files {
                self.open_path(f);
            }
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }

        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_quit && self.docs.iter().any(Doc::dirty) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.exit(&ctx);
        }

        ctx.request_repaint_after(std::time::Duration::from_secs(5));
        if self.now >= self.next_disk_check && !self.has_modal() {
            self.next_disk_check = self.now + 5.0;
            for i in 0..self.docs.len() {
                self.check_disk(i);
                if self.file_prompt.is_some() {
                    break;
                }
            }
        }
        self.dismiss_selection_tools(&ctx);
        let modal_open = self.has_modal();
        let searching = self.selection_tools.as_ref().is_some_and(|tools| tools.emoji)
            && ctx.memory(|memory| memory.has_focus(Id::new(EMOJI_SEARCH_ID)));
        let mut acts = vec![];
        if !modal_open && !searching {
            for &(m, k, a) in shortcuts() {
                if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k))) {
                    acts.push(a);
                }
            }
        }
        for a in acts.drain(..) {
            self.run(&ctx, a);
        }
        if !modal_open && !searching {
            self.list_keys(&ctx);
        }
        if !searching {
            self.detect_image_paste(&ctx);
        }
        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom != 1.0 && !modal_open {
            self.cfg.size = ((self.cfg.size * zoom * 2.0).round() / 2.0).clamp(8.0, 48.0);
        }

        Panel::top("menu")
            .frame(egui::Frame::NONE.fill(CHROME).inner_margin(Margin::symmetric(8, 4)))
            .show(ui, |ui| self.menu_bar(ui, &mut acts));
        Panel::bottom("status")
            .frame(egui::Frame::NONE.fill(STATUS).inner_margin(Margin::symmetric(12, 4)))
            .show(ui, |ui| self.status_bar(ui));
        if self.cfg.files {
            let browser = self.browser.get_or_insert_with(|| {
                let dir = self.docs.get(self.active).and_then(Doc::dir);
                files::Browser::new(dir.or_else(|| std::env::current_dir().ok()).unwrap_or_default())
            });
            let open: Vec<&Path> = self.docs.iter().filter_map(|d| d.path.as_deref()).collect();
            let active = self.docs.get(self.active).and_then(|d| d.path.as_deref());
            let picked = Panel::left("files")
                .resizable(true)
                .default_size(220.0)
                .frame(egui::Frame::NONE.fill(CHROME).inner_margin(Margin::symmetric(8, 4)))
                .show(ui, |ui| browser.ui(ui, &open, active, self.cfg.lang))
                .inner;
            match picked {
                Some(files::Event::Open(p)) => self.open_path(p),
                Some(files::Event::Close) => self.cfg.files = false,
                Some(files::Event::New) => self.new_file = Some(String::new()),
                Some(files::Event::Delete(p)) => self.delete_file = Some(p),
                None => {}
            }
        }
        Panel::top("tabs")
            .frame(egui::Frame::NONE.fill(CHROME).inner_margin(Margin { left: 6, right: 6, top: 4, bottom: 0 }))
            .show(ui, |ui| self.tabs(ui));
        let (link, middle) = CentralPanel::default()
            .frame(egui::Frame::NONE.fill(md::BG).inner_margin(Margin::symmetric(8, 4)))
            .show(ui, |ui| self.editor(ui))
            .inner;
        if let Some(url) = link {
            match url.strip_prefix('^') {
                Some(id) => self.jump_footnote(&ctx, id),
                None => self.open_link(&url),
            }
        }
        if let Some(c) = middle {
            self.set_selection(&ctx, c, c);
            self.run(&ctx, Action::PastePrimary);
        }
        self.sync_primary(&ctx);
        self.selection_popup(&ctx, &mut acts);
        self.modals(&ctx);
        for a in acts {
            self.run(&ctx, a);
        }

        self.renumber_lists(&ctx);
        let now = self.now;
        if let Some(doc) = self.docs.get_mut(self.active)
            && doc.record(now)
        {
            // The status bar was drawn before this edit; show the new numbers in the next frame.
            ctx.request_repaint();
        }

        let title = match self.docs.get(self.active) {
            Some(d) => format!("{} — {}", d.title(self.cfg.lang), crate::APP_TITLE),
            None => crate::APP_TITLE.to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        let (cfg, applied) = (&self.cfg, &self.applied_font);
        if (&applied.0, applied.1, applied.2, applied.3) != (&cfg.font, cfg.weight, cfg.bold_weight, cfg.lang) {
            fonts::apply(&ctx, &cfg.font, cfg.weight, cfg.bold_weight, cfg.lang);
            self.applied_font = (cfg.font.clone(), cfg.weight, cfg.bold_weight, cfg.lang);
        }
        if self.cfg.gamma != self.applied_gamma {
            self.applied_gamma = self.cfg.gamma;
            let g = self.applied_gamma;
            ctx.style_mut_of(egui::Theme::Dark, |s| {
                s.visuals.text_options.color_transfer_function = FontColorTransferFunction::Gamma(g);
            });
        }
        self.save_config_soon(&ctx);
    }

    fn on_exit(&mut self) {
        if self.cfg != self.saved_cfg {
            self.cfg.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_open_and_external_changes_preserve_user_choices() {
        let dir = std::env::temp_dir().join(format!("mdedit-disk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("external.md");
        std::fs::write(&path, "original\r\n").unwrap();
        let (mut app, _) = app_with("");
        app.open_path(path.clone());
        app.docs[0].text = "local\n".into();
        app.open_path(dir.join("./external.md"));
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.file_prompt, Some((app.docs[0].id, false)));
        assert!(app.save(0, false));
        app.file_prompt = None;
        app.check_disk(0);
        assert!(app.file_prompt.is_none());
        std::fs::write(&path, "outside\r\n").unwrap();
        app.check_disk(0);
        assert_eq!(app.file_prompt, Some((app.docs[0].id, true)));
        assert_eq!(app.docs[0].text, "local\n");
        app.file_prompt = None; // Do nothing acknowledges this version only.
        app.check_disk(0);
        assert!(app.file_prompt.is_none());
        std::fs::write(&path, "newer\r\n").unwrap();
        app.check_disk(0);
        assert!(app.file_prompt.is_some());
        assert!(app.reload_doc(0));
        assert_eq!(app.docs[0].text, "newer\n");
        assert!(!app.docs[0].dirty());
        assert!(app.docs[0].crlf);
        app.docs[0].step(0.0, false);
        assert_eq!(app.docs[0].text, "local\n");
        assert!(app.docs[0].dirty());
        app.file_prompt = None;
        app.check_disk(0);
        assert!(app.file_prompt.is_none());
        assert!(app.save(0, false));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "local\r\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn linked_page_shortcut_enter_creates_file_and_followable_link() {
        let dir = std::env::temp_dir().join(format!("mdedit-linked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut app, ctx) = app_with("Zażółć [tekst]");
        app.docs[0].path = Some(dir.join("source.md"));
        frame(&mut app, &ctx, vec![]);
        let end = app.docs[0].text.chars().count();
        app.set_selection(&ctx, 0, end);
        frame(&mut app, &ctx, vec![key(Key::K, Modifiers::COMMAND | Modifiers::SHIFT)]);
        assert_eq!(app.docs[0].text, "Zażółć [tekst]");
        app.new_page.as_mut().unwrap().name = "Żółw (test) #1 %20".into();
        frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(app.new_page.is_none());
        let target = dir.join("Żółw (test) #1 %20.md");
        assert!(target.is_file());
        let (_, decorations) = layout(&app.docs[0].text, None, 600.0);
        let url = decorations
            .iter()
            .find_map(|d| match d {
                Deco::Link { url, .. } => Some(url),
                _ => None,
            })
            .unwrap();
        assert_eq!(resolve(Some(&dir), url), Some(target.clone()));
        app.open_link(url);
        assert_eq!(app.docs[app.active].path.as_ref(), Some(&target));
        app.active = 0;
        app.undo_redo(&ctx, false);
        assert_eq!(app.docs[0].text, "Zażółć [tekst]");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn linked_page_preserves_existing_files_and_rejects_paths() {
        let dir = std::env::temp_dir().join(format!("mdedit-linked-refuse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("existing.md"), "keep").unwrap();
        let (mut app, ctx) = app_with("text");
        app.docs[0].path = Some(dir.join("source.md"));
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 0, 4);
        app.begin_new_page(&ctx);
        for name in ["existing.md", "../outside", "sub/file", "sub\\file", "", ".", "bad:name"] {
            app.new_page.as_mut().unwrap().name = name.into();
            assert!(app.create_linked_page(&ctx).is_err(), "{name}");
            assert_eq!(app.docs[0].text, "text");
        }
        assert_eq!(std::fs::read_to_string(dir.join("existing.md")).unwrap(), "keep");
        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(app.new_page.is_none());
        assert_eq!(app.docs[0].text, "text");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn linked_page_uses_browser_folder_for_unsaved_document() {
        let dir = std::env::temp_dir().join(format!("mdedit-linked-unsaved-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut app, ctx) = app_with("");
        app.browser = Some(files::Browser::new(dir.clone()));
        frame(&mut app, &ctx, vec![]);
        app.begin_new_page(&ctx);
        app.new_page.as_mut().unwrap().name = "new page".into();
        app.create_linked_page(&ctx).unwrap();
        assert_eq!(app.docs[0].text, "[new page](new%20page.md)");
        assert!(dir.join("new page.md").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn popup_output(app: &mut App, ctx: &Context, events: Vec<Event>) -> egui::FullOutput {
        let mut ef = eframe::Frame::_new_kittest();
        let mut output = ctx.run_ui(
            RawInput {
                events,
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                ..Default::default()
            },
            |ui| app.ui(ui, &mut ef),
        );
        output.textures_delta.clear();
        output
    }

    fn click_at(app: &mut App, ctx: &Context, pos: Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn selection_popup_is_contextual_anchored_and_dismissed_until_reselection() {
        let text = format!("{}hello world", "line\n".repeat(12));
        let (mut app, ctx) = app_with(&text);
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
        app.set_selection(&ctx, 60, 65);
        frame(&mut app, &ctx, vec![]);
        let output = popup_output(&mut app, &ctx, vec![]);
        let tools = app.selection_tools.as_ref().unwrap();
        assert!(!tools.emoji);
        let rects = tool_rects(&output, tools.rect, TOOL_BUTTON);
        assert_eq!(rects.len(), formatting_buttons().len());
        assert!(((rects[0].left() - tools.rect.left()) - (tools.rect.right() - rects[3].right())).abs() < 1.0);
        assert!(rects.iter().all(|rect| (rect.width() - rect.height()).abs() < 0.1));
        assert!((tools.rect.left() - tools.selection.anchor.x).abs() < 2.0);
        assert!((tools.rect.bottom() - (tools.selection.anchor.y - 6.0)).abs() < 2.0);
        let mut rows = std::collections::BTreeMap::new();
        for shape in &output.shapes {
            if let Shape::Text(text) = &shape.shape
                && ["B", "I", "S", "<>", "P", "H1", "H2", "H3"].contains(&text.galley.job.text.as_str())
            {
                *rows.entry(text.pos.y.round() as i32).or_insert(0) += 1;
            }
        }
        assert!(rows.values().all(|count| *count <= TOOL_COLUMNS));
        click_at(&mut app, &ctx, pos2(8.0, 400.0));
        assert!(app.selection_tools.is_none());
        assert_eq!(app.selection(&ctx), Some((60, 65)));
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
        app.set_selection(&ctx, 60, 66);
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_some());
        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(app.selection_tools.is_none());
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
        app.set_selection(&ctx, 61, 67);
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_some());
        app.about = true;
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!app.about);
        assert!(app.selection_tools.is_none());
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
        app.set_selection(&ctx, 62, 68);
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_some());
        app.docs[0].set_syntax(SyntaxMode::Language(Syntax::Html));
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
        app.new_doc();
        frame(&mut app, &ctx, vec![]);
        assert!(app.selection_tools.is_none());
    }

    fn emoji_tiles(output: &egui::FullOutput, ctx: &Context) -> Vec<(char, Rect)> {
        let textures: HashMap<_, _> = ctx.data(|data| {
            crate::emoji::characters()
                .iter()
                .filter_map(|ch| data.get_temp::<TextureHandle>(Id::new(("color_emoji", *ch))).map(|t| (t.id(), *ch)))
                .collect()
        });
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Mesh(mesh) => textures.get(&mesh.texture_id).map(|ch| (*ch, mesh.calc_bounds())),
                _ => None,
            })
            .collect()
    }

    fn tool_rects(output: &egui::FullOutput, popup: Rect, button_size: Vec2) -> Vec<Rect> {
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Rect(rect)
                    if popup.contains_rect(rect.rect) && (rect.rect.size() - button_size).length() < 0.1 =>
                {
                    Some(rect.rect)
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn larger_emoji_popup_scrolls_and_inserts_unicode_with_undo() {
        let text = format!("{}Zażółć", "line\n".repeat(12));
        let (mut app, ctx) = app_with(&text);
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 60, 66);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![]);
        let format_rect = app.selection_tools.as_ref().unwrap().rect;
        let output = popup_output(&mut app, &ctx, vec![key(Key::Backtick, Modifiers::COMMAND | Modifiers::SHIFT)]);
        let tools = app.selection_tools.as_ref().unwrap();
        assert!(tools.emoji);
        assert!(tools.rect.width() > format_rect.width());
        assert!(tools.rect.height() > format_rect.height());
        assert!((tools.rect.left() - format_rect.left()).abs() < 1.0);
        assert!((tools.rect.bottom() - format_rect.bottom()).abs() < 1.0);
        let first = crate::emoji::characters()[0].to_string();
        let tiles = emoji_tiles(&output, &ctx);
        let button = tiles.iter().find(|(ch, _)| ch.to_string() == first).expect("first emoji is visible").1.center();
        assert!(tiles.len() < crate::emoji::characters().len());
        let rects = tool_rects(&output, tools.rect, EMOJI_BUTTON);
        assert!(rects.len() >= 8);
        for row in rects.chunks(TOOL_COLUMNS).take(2) {
            assert!(row.iter().all(|rect| (rect.top() - row[0].top()).abs() < 0.1));
        }
        let left = rects[0].left() - tools.rect.left();
        let right = tools.rect.right() - rects[TOOL_COLUMNS - 1].right();
        assert!((left - right).abs() < 1.0, "unequal margins: {left} / {right}");
        click_at(&mut app, &ctx, button);
        assert_eq!(app.docs[0].text, format!("{}{first}", "line\n".repeat(12)));
        assert_eq!(app.selection(&ctx), Some((61, 61)));
        assert!(app.selection_tools.is_none());
        app.run(&ctx, Action::Undo);
        assert_eq!(app.docs[0].text, text);
        app.set_selection(&ctx, 60, 60);
        frame(&mut app, &ctx, vec![key(Key::Backtick, Modifiers::COMMAND)]);
        assert!(app.selection_tools.as_ref().is_some_and(|tools| tools.emoji));
        let popup = app.selection_tools.as_ref().unwrap().rect;
        frame(&mut app, &ctx, vec![Event::PointerMoved(popup.center())]);
        frame(
            &mut app,
            &ctx,
            vec![Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                phase: egui::TouchPhase::Move,
                delta: vec2(0.0, -240.0),
                modifiers: Modifiers::NONE,
            }],
        );
        for _ in 0..8 {
            frame(&mut app, &ctx, vec![]);
        }
        let scrolled = popup_output(&mut app, &ctx, vec![]);
        let tiles = emoji_tiles(&scrolled, &ctx);
        assert!(!tiles.iter().any(|(ch, _)| ch.to_string() == first));
        let (chosen, position) = tiles
            .iter()
            .find(|(_, rect)| popup.shrink(20.0).contains(rect.center()))
            .map(|(ch, rect)| (ch.to_string(), rect.center()))
            .expect("scrolled emoji is visible");
        click_at(&mut app, &ctx, position);
        assert_eq!(app.docs[0].text, format!("{}{chosen}Zażółć", "line\n".repeat(12)));
        app.run(&ctx, Action::Undo);
        assert_eq!(app.docs[0].text, text);
        frame(&mut app, &ctx, vec![key(Key::Backtick, Modifiers::COMMAND)]);
        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(app.selection_tools.is_none());
    }

    #[test]
    fn emoji_search_filters_live_without_editing_the_source_selection() {
        let text = format!("{}Zażółć", "line\n".repeat(12));
        let (mut app, ctx) = app_with(&text);
        app.cfg.lang = Lang::Polish;
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 60, 66);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![key(Key::Backtick, Modifiers::COMMAND)]);
        let output = popup_output(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.has_focus(Id::new(EMOJI_SEARCH_ID))));
        let popup = app.selection_tools.as_ref().unwrap().rect;
        let search = ctx.read_response(Id::new(EMOJI_SEARCH_ID)).unwrap().rect;
        assert!((search.width() - emoji_content_size().x).abs() < 1.0);
        let buttons = tool_rects(&output, popup, EMOJI_BUTTON);
        assert!(search.bottom() < buttons[0].top());
        assert!((search.left() - buttons[0].left()).abs() < 1.0);
        let undo = app.docs[0].undo.clone();
        frame(&mut app, &ctx, vec![Event::Text("SeRcE".into())]);
        let filtered = popup_output(&mut app, &ctx, vec![]);
        assert_eq!(app.selection_tools.as_ref().unwrap().query, "SeRcE");
        let tiles = emoji_tiles(&filtered, &ctx);
        let matches = crate::emoji::search("serce", Lang::Polish);
        assert!(!tiles.is_empty());
        assert!(tiles.iter().all(|(ch, _)| matches.contains(ch)));
        for key_event in [key(Key::B, Modifiers::COMMAND), key(Key::Enter, Modifiers::NONE)] {
            frame(&mut app, &ctx, vec![key_event]);
            assert_eq!(app.docs[0].text, text);
            assert_eq!(app.selection_tools.as_ref().unwrap().selection.range, (60, 66));
        }
        let empty = popup_output(
            &mut app,
            &ctx,
            vec![key(Key::A, Modifiers::COMMAND), Event::Text("no_such_emoji_92329".into())],
        );
        assert!(emoji_tiles(&empty, &ctx).is_empty());
        assert!(
            empty
                .shapes
                .iter()
                .any(|s| matches!(&s.shape, Shape::Text(t) if t.galley.job.text == "Nie znaleziono emoji"))
        );
        assert_eq!(app.selection_tools.as_ref().unwrap().rect, popup);
        assert_eq!(app.docs[0].undo, undo);
        let output =
            popup_output(&mut app, &ctx, vec![key(Key::A, Modifiers::COMMAND), Event::Text("czerwone serce".into())]);
        let heart = emoji_tiles(&output, &ctx)
            .into_iter()
            .find(|(ch, _)| *ch == '❤')
            .expect("matching heart is visible")
            .1
            .center();
        click_at(&mut app, &ctx, heart);
        assert_eq!(app.docs[0].text, format!("{}❤", "line\n".repeat(12)));
        assert!(app.selection_tools.is_none());
        app.run(&ctx, Action::Undo);
        assert_eq!(app.docs[0].text, text);
        frame(&mut app, &ctx, vec![key(Key::Backtick, Modifiers::COMMAND)]);
        assert!(app.selection_tools.as_ref().unwrap().query.is_empty());
        frame(&mut app, &ctx, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(app.selection_tools.is_none());
        assert!(ctx.memory(|memory| memory.has_focus(app.docs[0].edit_id())));
    }

    #[test]
    fn clicking_selection_tools_keeps_editor_selection() {
        let (mut app, ctx) = app_with("hello world");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 0, 5);
        frame(&mut app, &ctx, vec![]);
        let mut ef = eframe::Frame::_new_kittest();
        let mut output = ctx.run_ui(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))), ..Default::default() },
            |ui| app.ui(ui, &mut ef),
        );
        output.textures_delta.clear();
        let button = output
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                Shape::Text(text) if text.galley.job.text == "B" => Some(text.pos + text.galley.size() * 0.5),
                _ => None,
            })
            .unwrap();
        output.textures_delta.clear();
        for pressed in [true, false] {
            frame(
                &mut app,
                &ctx,
                vec![
                    Event::PointerMoved(button),
                    Event::PointerButton {
                        pos: button,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
        assert_eq!(app.docs[0].text, "**hello** world");
    }

    #[test]
    fn selection_tools_cover_all_formatting_and_numbered_lists() {
        let buttons = formatting_buttons();
        for (_, _, action) in shortcuts() {
            if matches!(action, Action::Fmt(_) | Action::Insert | Action::NewPage) {
                assert!(buttons.iter().any(|(_, _, a)| a == action));
            }
        }
        assert!(buttons.iter().any(|(_, _, a)| *a == Action::Fmt(Fmt::Ordered)));
        let (mut app, ctx) = app_with("one\ntwo");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 0, 7);
        app.run(&ctx, Action::Fmt(Fmt::Ordered));
        assert_eq!(app.docs[0].text, "1. one\n2. two");
        app.run(&ctx, Action::Fmt(Fmt::Ordered));
        assert_eq!(app.docs[0].text, "one\ntwo");
    }

    #[test]
    fn quote_format_preserves_indentation_and_toggles_back() {
        let text = "  quoted\n\t\tdeeper";
        let (mut app, ctx) = app_with(text);
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 0, text.chars().count());
        app.run(&ctx, Action::Fmt(Fmt::Quote));
        assert_eq!(app.docs[0].text, "  > quoted\n\t\t> deeper");
        app.run(&ctx, Action::Fmt(Fmt::Quote));
        assert_eq!(app.docs[0].text, text);
    }

    #[test]
    fn indented_quotes_align_bars_text_and_wrapped_rows() {
        for (prefix, indent, quotes) in
            [("> ", 0, 1), ("  > ", 1, 1), ("\t> ", 1, 1), ("    > > ", 2, 2), ("\t\t> ", 2, 1)]
        {
            let text = format!("{prefix}{}", "word ".repeat(30));
            for cursor in [None, Some(0)] {
                let (galley, decos) = layout(&text, cursor, 200.0);
                assert!(decos.iter().any(|d| matches!(d, Deco::Quote { indent: i, level, .. }
                    if *i == indent && *level == quotes)));
                let expected = (indent + quotes) as f32 * LOOK.indent();
                let x = galley.pos_from_cursor(CCursor::new(prefix.chars().count())).min.x;
                assert!((x - expected).abs() < 1.0, "{prefix:?}: {x} vs {expected}");
                for row in &galley.rows[1..] {
                    assert!((row.pos.x - expected).abs() < 1.0);
                }
            }
        }
        let (_, decos) = layout("- item\n  > quote\n    > [!note] nested callout", None, 500.0);
        assert!(decos.iter().any(|d| matches!(d, Deco::Quote { indent: 1, level: 1, .. })));
        assert!(decos.iter().any(|d| matches!(d, Deco::Quote { indent: 2, color: Some(_), .. })));
    }

    #[test]
    fn numbering_and_layout_agree_on_odd_indents_and_long_fences() {
        let text = "1. parent\n  1. first\n   9. second\n\t8. third";
        let mut normalized = text.to_owned();
        for (a, b, replacement) in numbered_edits(text).into_iter().rev() {
            normalized.replace_range(a..b, &replacement);
        }
        assert_eq!(normalized, "1. parent\n  1. first\n   2. second\n\t3. third");
        let (g, _) = layout(&normalized, None, 500.0);
        let mut offset = 0;
        for (i, line) in normalized.lines().enumerate() {
            let parsed = markdown::prefix(line);
            if i > 0 {
                let x = g.pos_from_cursor(CCursor::new(offset + parsed.content_start)).min.x;
                assert!((x - 2.0 * LOOK.indent()).abs() < 1.0);
            }
            offset += line.len() + 1;
        }
        let code = "````md\n1. code\n```\n9. still code\n````\n1. item\n9. item";
        let edits = numbered_edits(code);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].0, code.rfind("9.").unwrap());
        let (_, decos) = layout(code, None, 500.0);
        assert_eq!(decos.iter().filter(|d| matches!(d, Deco::Code(_, _))).count(), 1);
        for header in ["---\n1. header\n9. header\n...\n", "---\n1. header\n9. header\n---\n"] {
            let document = format!("{header}1. item\n9. item");
            assert_eq!(numbered_edits(&document).len(), 1);
        }
        assert_eq!(numbered_edits("---\n1. item\n9. item").len(), 1);
    }

    #[test]
    fn browser_creation_reuses_filename_validation_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("mdedit-shared-create-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("existing.md"), "keep").unwrap();
        let (mut app, _ctx) = app_with("");
        app.browser = Some(files::Browser::new(dir.clone()));
        for name in ["../escape", r"..\escape", "a/b", "bad:name"] {
            app.new_file = Some(name.into());
            app.create_file();
            assert!(app.docs[0].path.is_none());
        }
        app.new_file = Some("existing.md".into());
        app.create_file();
        assert!(app.error.is_some());
        assert_eq!(std::fs::read_to_string(dir.join("existing.md")).unwrap(), "keep");
        app.new_file = Some("new page".into());
        app.create_file();
        assert!(dir.join("new page.md").is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[ignore = "manual release-mode latency benchmark"]
    fn benchmark_editor_syntax_latency() {
        use std::time::Instant;
        let sample = "if [[ -n $HOME ]]; then printf '%s' \"$HOME\"; fi # żółw🙂\n";
        for bytes in [10_000, 100_000, 1_000_000] {
            let text = sample.repeat(bytes / sample.len() + 1);
            let (mut app, ctx) = app_with(&text);
            app.docs[0].syntax = Some(Syntax::Bash);
            app.cfg.files = false;
            app.cfg.inline_title = false;
            let start = Instant::now();
            frame(&mut app, &ctx, vec![]);
            let cold = start.elapsed();
            let mut times = Vec::new();
            for index in 0..40 {
                let c = sample.chars().count() * index;
                app.set_selection(&ctx, c, c);
                let start = Instant::now();
                frame(&mut app, &ctx, vec![]);
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            let (p95, p99) = (times[37], times[39]);
            app.set_selection(&ctx, text.chars().count() / 2, text.chars().count() / 2);
            let start = Instant::now();
            frame(&mut app, &ctx, vec![Event::Text("x".into())]);
            eprintln!(
                "editor bytes={} cold_ms={:.3} cursor_p95_ms={p95:.3} cursor_p99_ms={p99:.3} edit_ms={:.3}",
                text.len(),
                cold.as_secs_f64() * 1000.0,
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn manual_syntax_is_tab_local_and_preserves_text_history_and_save() {
        let text = "// comment\nfn main() {}";
        let (mut app, ctx) = app_with(text);
        frame(&mut app, &ctx, vec![]);
        let original = app.docs[0].text.clone();
        let undo = app.docs[0].undo.clone();
        let dirty = app.docs[0].dirty();
        app.docs[0].set_syntax(SyntaxMode::Language(Syntax::Rust));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            frame(&mut app, &ctx, vec![]);
            if app.docs[0].analysis.spans().iter().any(|s| s.range.contains(&3) && s.color == md::DIM) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "manual Rust coloring did not arrive");
        }
        let revision = app.docs[0].analysis.revision();
        app.docs[0].set_syntax(SyntaxMode::Markdown);
        frame(&mut app, &ctx, vec![]);
        assert!(app.docs[0].analysis.revision() > revision);
        assert!(app.docs[0].analysis.spans().is_empty());
        assert_eq!(app.docs[0].syntax, None);
        assert_eq!(app.docs[0].text, original);
        assert_eq!(app.docs[0].undo, undo);
        assert_eq!(app.docs[0].dirty(), dirty);

        let path = std::env::temp_dir().join(format!("mdedit-manual-syntax-{}.html", std::process::id()));
        app.docs[0].path = Some(path.clone());
        app.docs[0].set_syntax(SyntaxMode::Language(Syntax::Bash));
        assert!(app.save(0, false));
        assert_eq!(app.docs[0].syntax, Some(Syntax::Bash));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        app.new_doc();
        assert_eq!(app.docs[1].syntax_mode, SyntaxMode::Automatic);
        assert_eq!(app.docs[1].syntax, None);
        assert_eq!(app.docs[0].syntax, Some(Syntax::Bash));
        app.docs[0].set_syntax(SyntaxMode::Automatic);
        assert_eq!(app.docs[0].syntax, Some(Syntax::Html));
        // The same refresh is used after Save As; explicit Markdown survives a new extension.
        app.docs[0].set_syntax(SyntaxMode::Markdown);
        app.docs[0].path = Some(path.with_extension("rs"));
        let mode = app.docs[0].syntax_mode;
        app.docs[0].set_syntax(mode);
        assert_eq!(app.docs[0].syntax, None);
        app.docs[0].set_syntax(SyntaxMode::Automatic);
        assert_eq!(app.docs[0].syntax, Some(Syntax::Rust));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn code_cursor_and_wrapping_do_not_reanalyze_syntax() {
        let (mut app, ctx) = app_with("echo first\necho second");
        app.docs[0].syntax = Some(Syntax::Bash);
        frame(&mut app, &ctx, vec![]);
        let galley = app.docs[0].cache.as_ref().unwrap().galley.clone();
        let revision = app.docs[0].analysis.revision();
        app.set_selection(&ctx, 12, 12);
        frame(&mut app, &ctx, vec![]);
        assert!(Arc::ptr_eq(&galley, &app.docs[0].cache.as_ref().unwrap().galley));
        let mut f = eframe::Frame::_new_kittest();
        ctx.run_ui(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 800.0))), ..Default::default() },
            |ui| app.ui(ui, &mut f),
        )
        .textures_delta
        .clear();
        assert_eq!(app.docs[0].analysis.revision(), revision);
        assert!(!Arc::ptr_eq(&galley, &app.docs[0].cache.as_ref().unwrap().galley));
    }

    #[test]
    fn dropped_html_opens_and_colors_long_lines() {
        let dir = std::env::temp_dir().join(format!("mdedit-drop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page.html");
        let text =
            format!("<!doctype html><html><body><!-- comment -->{}<b>hello</b></body></html>", "text ".repeat(4500));
        std::fs::write(&path, &text).unwrap();
        let (mut app, ctx) = app_with("");
        #[derive(Debug)]
        struct DropPath(PathBuf);
        impl egui::DroppedFile for DropPath {
            fn path(&self) -> &std::path::Path {
                &self.0
            }
            fn bytes(&self) -> Result<Vec<u8>, String> {
                std::fs::read(&self.0).map_err(|e| e.to_string())
            }
        }
        let input = RawInput {
            dropped_files: vec![
                Arc::new(DropPath(dir.clone())),
                Arc::new(DropPath(path.clone())),
                Arc::new(DropPath(path.clone())),
            ],
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        ctx.run_ui(input, |ui| app.ui(ui, &mut f)).textures_delta.clear();
        assert_eq!(app.docs.len(), 1);
        assert_eq!(app.docs[0].path.as_ref(), Some(&path));
        assert_eq!(app.docs[0].syntax, Some(Syntax::Html));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.docs[0].analysis.spans().is_empty() {
            assert!(std::time::Instant::now() < deadline, "HTML analysis did not complete");
            frame(&mut app, &ctx, vec![]);
        }
        let byte = text.find("comment").unwrap();
        assert!(
            app.docs[0]
                .cache
                .as_ref()
                .unwrap()
                .galley
                .job
                .sections
                .iter()
                .any(|s| s.byte_range.contains(&egui::text::ByteIndex(byte)) && s.format.color == md::DIM)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn asynchronous_fenced_colors_reach_markdown_layout() {
        let text = "żółw🙂\n```rust\n// code-comment\nfn demo() {}\n```\n";
        let (mut app, ctx) = app_with(text);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while app.docs[0].analysis.spans().is_empty() {
            assert!(std::time::Instant::now() < deadline, "syntax worker did not complete");
            frame(&mut app, &ctx, vec![]);
        }
        let byte = text.find("code-comment").unwrap();
        let galley = &app.docs[0].cache.as_ref().unwrap().galley;
        assert!(
            galley
                .job
                .sections
                .iter()
                .any(|s| s.byte_range.contains(&egui::text::ByteIndex(byte)) && s.format.color == md::DIM)
        );
    }

    #[test]
    #[ignore = "manual release-mode nested-language editor benchmark"]
    fn benchmark_nested_editor_latency() {
        use std::time::Instant;
        let sample = "<style>body { color: red; }</style>\n<script>const message = 'żółw🙂'; console.log(message);</script>\n<div><?php echo $name; ?></div>\n";
        for bytes in [10_000, 100_000, 1_000_000] {
            let text = sample.repeat(bytes / sample.len() + 1);
            let (mut app, ctx) = app_with(&text);
            app.docs[0].syntax = Some(Syntax::Php);
            app.cfg.files = false;
            app.cfg.inline_title = false;
            let start = Instant::now();
            frame(&mut app, &ctx, vec![]);
            let first = start.elapsed().as_secs_f64() * 1000.0;
            while app.docs[0].analysis.spans().is_empty() {
                frame(&mut app, &ctx, vec![]);
            }
            let colored = start.elapsed().as_secs_f64() * 1000.0;
            let mut times = Vec::new();
            for i in 0..40 {
                let at = sample.chars().count() * i;
                app.set_selection(&ctx, at, at);
                let start = Instant::now();
                frame(&mut app, &ctx, vec![]);
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            app.set_selection(&ctx, text.chars().count() / 2, text.chars().count() / 2);
            let start = Instant::now();
            frame(&mut app, &ctx, vec![Event::Text("x".into())]);
            let first_edit = start.elapsed().as_secs_f64() * 1000.0;
            let revision = app.docs[0].analysis.revision();
            while app.docs[0].analysis.revision() == revision {
                frame(&mut app, &ctx, vec![]);
            }
            eprintln!(
                "nested_editor bytes={} first_frame_ms={first:.3} open_to_colors_ms={colored:.3} cursor_p95_ms={:.3} cursor_p99_ms={:.3} edit_first_frame_ms={first_edit:.3} edit_to_colors_ms={:.3}",
                text.len(),
                times[37],
                times[39],
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn width_modes_cycle_persist_and_migrate_old_settings() {
        let (mut app, ctx) = app_with("text");
        frame(&mut app, &ctx, vec![]);
        for expected in [1, 2, 3, 0] {
            frame(
                &mut app,
                &ctx,
                vec![
                    Event::Key {
                        key: Key::F2,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                    Event::Key {
                        key: Key::F2,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            assert_eq!(app.cfg.view_width, expected);
            assert!(Config::default_text().parse(&app.cfg.to_text()) == app.cfg);
        }
        assert_eq!(Config::default_text().parse("optimize=0").view_width, 3);
        assert_eq!(Config::default_text().parse("optimize=1").view_width, 0);
        assert_eq!(Config::default_text().parse("view_width=2\noptimize=0").view_width, 2);
        assert_eq!(Config::default_text().parse("view_width=99").view_width, 0);
        for gutter in [0.0, 48.0] {
            let modes: Vec<_> = (0..4).map(|i| view_geometry(1500.0, gutter, i)).collect();
            assert_eq!(modes[0].0, MAX_LINE_W);
            assert_eq!(modes[3], (1500.0 - gutter - 48.0, gutter + 24.0));
            assert!(modes.windows(2).all(|pair| pair[0].0 < pair[1].0 && pair[0].1 > pair[1].1));
            for i in 0..4 {
                let (width, left) = view_geometry(400.0, gutter, i);
                assert!(width >= 100.0 && left >= 0.0 && width + left <= 400.0);
            }
        }
    }

    #[test]
    fn settings_round_trip_and_reject_bad_values() {
        let custom = Config { lang: Lang::German, size: 20.5, weight: 400, files: true, ..Config::default_text() };
        assert!(custom == Config::default_text().parse(&custom.to_text()));
        let broken =
            Config::default_text().parse("size=0\nweight=5000\ngamma=nan\nline_height=abc\nunknown=1\nlang=xx");
        assert!(broken == Config::default_text(), "invalid values keep the defaults");
        assert_eq!(Config::default_text().parse("pl=1").lang, Lang::Polish, "old settings files still work");
    }

    #[test]
    fn replacement_keeps_numbering_and_undo_together() {
        for all in [false, true] {
            let text = "1. one\n2. remove\n3. three";
            let (mut app, ctx) = app_with(text);
            frame(&mut app, &ctx, vec![]);
            app.run(&ctx, Action::Find);
            let f = app.find.as_mut().unwrap();
            f.query = "2. remove\n".into();
            f.replace = String::new();
            app.set_selection(&ctx, 7, 17);
            if all {
                app.replace_all(&ctx);
            } else {
                app.replace_one(&ctx);
            }
            assert_eq!(app.docs[0].text, "1. one\n2. three");
            app.undo_redo(&ctx, false);
            assert_eq!(app.docs[0].text, text);
        }
    }

    #[test]
    fn shift_tab_reverses_one_rendered_indentation_level() {
        let (mut app, ctx) = app_with("    - child");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 11, 11);
        frame(&mut app, &ctx, vec![key(Key::Tab, Modifiers::SHIFT)]);
        assert_eq!(app.docs[0].text, "  - child");
    }

    #[test]
    fn huge_images_are_scaled_to_fit_a_texture_and_absurd_ones_refused() {
        let ctx = Context::default();
        ctx.run_ui(RawInput { max_texture_side: Some(1024), ..Default::default() }, |_| {}).textures_delta.clear();
        let encode = |w, h| {
            let mut png = Vec::new();
            image::DynamicImage::new_rgb8(w, h)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            format!("data:image/png;base64,{}", base64::encode(&png))
        };
        let (_, size) = load_image(&ctx, None, &encode(9000, 30)).expect("a wide image loads");
        assert!(size.x <= 1024.0 && size.x > 0.0 && size.y >= 1.0);
        assert!(load_image(&ctx, None, &encode(20_000, 4)).is_none(), "wider than the decoder limit");
        ctx.run_ui(RawInput::default(), |_| {}).textures_delta.clear();
    }

    #[test]
    fn only_web_links_are_opened_and_never_programs() {
        assert_eq!(url_scheme("https://example.com"), Some("https"));
        assert_eq!(url_scheme("mailto:a@b.c"), Some("mailto"));
        assert_eq!(url_scheme("C:\\Users\\x.md"), None, "a drive letter is not a scheme");
        assert_eq!(url_scheme("notes/a.md"), None);
        assert!(runs_code(Path::new("setup.EXE")));
        assert!(runs_code(Path::new("run.sh")));
        assert!(!runs_code(Path::new("/nonexistent/photo.png")));
    }

    #[test]
    fn crlf_files_keep_their_line_endings() {
        let dir = std::env::temp_dir().join(format!("mdedit-crlf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.md");
        std::fs::write(&path, "one\r\ntwo\r\n").unwrap();
        let (mut app, _ctx) = app_with("");
        app.open_path(path.clone());
        assert_eq!(app.docs[0].text, "one\ntwo\n");
        app.docs[0].text.push_str("three");
        assert!(app.save(0, false));
        assert_eq!(std::fs::read(&path).unwrap(), b"one\r\ntwo\r\nthree");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn print_and_export_shortcuts_do_not_trigger_inline_code() {
        for (modifiers, key, expected) in [
            (Modifiers::COMMAND, Key::P, Action::Print),
            (Modifiers::COMMAND | Modifiers::SHIFT, Key::E, Action::ExportPdf),
            (Modifiers::COMMAND, Key::E, Action::Fmt(Fmt::Code)),
        ] {
            let ctx = Context::default();
            let mut actions = vec![];
            let mut output = ctx.run_ui(
                RawInput {
                    events: vec![Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }],
                    ..Default::default()
                },
                |ui| {
                    ui.input_mut(|input| {
                        for &(m, k, action) in shortcuts() {
                            if input.consume_shortcut(&KeyboardShortcut::new(m, k)) {
                                actions.push(action);
                            }
                        }
                    });
                },
            );
            output.textures_delta.clear();
            assert!(actions == [expected], "unexpected shortcut dispatch for {key:?}");
        }
    }

    #[test]
    fn about_lists_every_shortcut() {
        let covered: Vec<Action> = about_rows(Lang::English).into_iter().flat_map(|r| r.covers).collect();
        for &(m, k, a) in shortcuts() {
            assert!(covered.contains(&a), "About is missing {} ({})", shortcut_label(m, k), k.name());
        }
    }
    use eframe::App as _;
    use egui::{Event, RawInput};

    #[test]
    fn pdf_exports_unsaved_unicode_tables_and_multiple_pages_without_editing() {
        let source = format!(
            "# Zażółć gęślą jaźń 😀 ❤\n\n**Pogrubienie** i *kursywa*\n\n| Kolumna | Wartość |\n| --- | --- |\n| Szukaj | Żółw |\n\n- punkt\n- [x] gotowe\n\n> cytat\n\n```rust\nlet x = 42;\n```\n\n{}",
            (0..160).map(|i| format!("Wiersz{i:03} przeszukiwalny tekst\n")).collect::<String>()
        );
        let source = format!("{source}\n![icon]({}/assets/icon.png)\n", env!("CARGO_MANIFEST_DIR"));
        let (mut app, ctx) = app_with(&source);
        fonts::apply(&ctx, "", 400, 700, Lang::Polish);
        frame(&mut app, &ctx, vec![]);
        app.docs[0].saved = "older disk content".into();
        let undo = app.docs[0].undo.clone();
        let pdf = app.document_pdf(&ctx).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(pdf.windows(10).any(|w| w == b"/ToUnicode"));
        assert!(pdf.windows(6).any(|w| w == b"/Image"));
        assert_eq!(app.docs[0].text, source);
        assert_eq!(app.docs[0].saved, "older disk content");
        assert_eq!(app.docs[0].undo, undo);
        assert!(app.docs[0].dirty());
        // Poppler is optional for contributors; where installed it checks real
        // Unicode extraction and ensures clipped pages don't duplicate text.
        let dir = std::env::temp_dir().join(format!("mdedit-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("document.pdf");
        std::fs::write(&path, pdf).unwrap();
        match std::process::Command::new("pdftotext").arg(&path).arg("-").output() {
            Ok(output) => {
                assert!(output.status.success());
                let text = String::from_utf8(output.stdout).unwrap();
                assert!(text.contains("Zażółć gęślą jaźń"), "{text}");
                assert_eq!(text.matches('😀').count(), 1, "emoji text layer must remain searchable");
                assert_eq!(text.matches('❤').count(), 1, "symbol text layer must remain searchable");
                assert!(text.contains("Kolumna") && text.contains("Żółw"), "{text}");
                assert!(!text.contains("**") && !text.contains("```"));
                for i in 0..160 {
                    assert_eq!(text.matches(&format!("Wiersz{i:03}")).count(), 1, "row {i}: {text}");
                }
                assert!(text.matches('\u{c}').count() >= 3);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("{e}"),
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pdf_handles_empty_and_source_documents() {
        let (mut app, ctx) = app_with("");
        frame(&mut app, &ctx, vec![]);
        assert!(app.document_pdf(&ctx).unwrap().starts_with(b"%PDF-"));
        app.docs[0].text = "<html>\n  <body>Zażółć &amp; literal</body>\n</html>".into();
        app.docs[0].syntax = Some(Syntax::Html);
        assert!(app.document_pdf(&ctx).unwrap().starts_with(b"%PDF-"));
    }

    fn app_with(text: &str) -> (App, Context) {
        let ctx = Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let (_tx, rx) = std::sync::mpsc::channel();
        let mut app = App::new(&cc, vec![], rx);
        app.docs[0].text = text.to_owned();
        (app, ctx)
    }

    fn frame(app: &mut App, ctx: &Context, events: Vec<Event>) {
        let input = RawInput {
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        ctx.run_ui(input, |ui| app.ui(ui, &mut f)).textures_delta.clear();
    }

    fn frame_at(app: &mut App, ctx: &Context, time: f64, events: Vec<Event>) {
        let input = RawInput {
            events,
            time: Some(time),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
            ..Default::default()
        };
        let mut f = eframe::Frame::_new_kittest();
        ctx.run_ui(input, |ui| app.ui(ui, &mut f)).textures_delta.clear();
    }

    #[test]
    fn undo_redo_groups_typing() {
        let (mut app, ctx) = app_with("a");
        app.docs[0] = Doc::new(1, None, "a".into());
        frame_at(&mut app, &ctx, 0.0, vec![]);
        for (t, text) in [(0.1, "ab"), (0.2, "abc"), (2.0, "abcd")] {
            app.docs[0].text = text.into();
            frame_at(&mut app, &ctx, t, vec![]);
        }
        let z = |m| vec![key(Key::Z, m)];
        frame_at(&mut app, &ctx, 3.0, z(Modifiers::COMMAND));
        assert_eq!(app.docs[0].text, "abc");
        frame_at(&mut app, &ctx, 3.1, z(Modifiers::COMMAND));
        assert_eq!(app.docs[0].text, "a");
        frame_at(&mut app, &ctx, 3.2, vec![key(Key::Y, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "abc");
        frame_at(&mut app, &ctx, 3.3, z(Modifiers::COMMAND | Modifiers::SHIFT));
        assert_eq!(app.docs[0].text, "abcd");
    }

    #[test]
    fn undo_history_is_limited() {
        let mut doc = Doc::new(1, None, String::new());
        for i in 0..15 {
            doc.text.push('x');
            doc.record(i as f64 * 2.0);
        }
        assert_eq!(doc.undo.len(), HISTORY);
    }

    #[test]
    fn binary_and_huge_files_are_refused() {
        let dir = std::env::temp_dir().join(format!("mdedit-refuse-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bin"), [0u8, 1, 2, 255]).unwrap();
        std::fs::write(dir.join("big.md"), vec![b'a'; files::MAX_SIZE as usize + 1]).unwrap();
        let (mut app, ctx) = app_with("");
        for name in ["a.bin", "big.md"] {
            app.error = None;
            app.open_path(dir.join(name));
            assert!(app.error.is_some(), "{name} should be refused");
            assert!(app.docs.iter().all(|d| d.path.is_none()), "{name} must not open");
            frame(&mut app, &ctx, vec![]);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn binary_files_need_confirmation() {
        let dir = std::env::temp_dir().join(format!("mdedit-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.bin"), [0u8, 1, 2, 255]).unwrap();
        std::fs::write(dir.join("a.md"), "zażółć").unwrap();
        assert!(files::open_warning(&dir.join("a.bin")).is_some());
        assert!(files::open_warning(&dir.join("a.md")).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_prefixes_continue() {
        let p = |l: &str| list_prefix(l).map(|(a, b, _)| (a, b));
        assert_eq!(p("- item"), Some(("".into(), "- ".into())));
        assert_eq!(p("\t- [x] done"), Some(("\t".into(), "- [ ] ".into())));
        assert_eq!(p("9. nine"), Some(("".into(), "10. ".into())));
        assert_eq!(p("> > quote"), Some(("> > ".into(), "".into())));
        assert_eq!(p("plain"), None);
    }

    #[test]
    fn enter_continues_and_ends_lists() {
        let (mut app, ctx) = app_with("- one");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 5, 5);
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(app.docs[0].text, "- one\n- ");
        frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(app.docs[0].text, "- one\n");
    }

    #[test]
    fn shift_enter_continues_text_and_enter_starts_next_item() {
        for marker in ["- ", "1. "] {
            let text = format!("{marker}żółw");
            let (mut app, ctx) = app_with(&text);
            frame(&mut app, &ctx, vec![]);
            let end = text.chars().count();
            app.set_selection(&ctx, end, end);
            frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::SHIFT)]);
            assert_eq!(app.docs[0].text, format!("{text}\n{}", " ".repeat(2)));
            frame(&mut app, &ctx, vec![Event::Text("ciąg dalszy".into())]);
            frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
            assert!(app.docs[0].text.ends_with(if marker == "- " { "\n- " } else { "\n2. " }));
        }
    }

    #[test]
    fn shift_enter_uses_one_compact_step_for_tasks_and_nested_lists() {
        for prefix in ["", "\t", "\t\t"] {
            for marker in ["- ", "12. ", "- [ ] ", "- [x] "] {
                let text = format!("{prefix}{marker}text");
                let (mut app, ctx) = app_with(&text);
                frame(&mut app, &ctx, vec![]);
                let end = app.docs[0].text.chars().count();
                app.set_selection(&ctx, end, end);
                for _ in 0..2 {
                    frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::SHIFT)]);
                    assert!(app.docs[0].text.ends_with(&format!("\n{prefix}  ")));
                    frame(&mut app, &ctx, vec![Event::Text("continued".into())]);
                }
            }
        }
        let (galley, _) = layout("- [ ] text\n  continued", None, 600.0);
        assert!((galley.rows[1].pos.x - LOOK.indent()).abs() < 1.0);
    }

    #[test]
    fn tab_nests_items_and_numbering_maps_unicode_cursor() {
        let (mut app, ctx) = app_with("9. żółw\n10. kot");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 15, 15);
        app.docs[0].text = "9. żółw\n9. kot".into();
        app.set_selection(&ctx, 14, 14);
        frame(&mut app, &ctx, vec![Event::Text("y".into())]);
        assert_eq!(app.docs[0].text, "9. żółw\n10. koty");
        assert_eq!(app.selection(&ctx), Some((16, 16)));
        frame(&mut app, &ctx, vec![key(Key::Tab, Modifiers::NONE)]);
        assert_eq!(app.docs[0].text, "9. żółw\n\t1. koty");
        frame(&mut app, &ctx, vec![key(Key::Tab, Modifiers::SHIFT)]);
        assert_eq!(app.docs[0].text, "9. żółw\n10. koty");
    }

    #[test]
    fn both_new_tab_shortcuts_work() {
        let (mut app, ctx) = app_with("text");
        frame(&mut app, &ctx, vec![]);
        for k in [Key::N, Key::T] {
            let count = app.docs.len();
            frame(&mut app, &ctx, vec![key(k, Modifiers::COMMAND)]);
            assert_eq!(app.docs.len(), count + 1);
            assert!(app.docs[app.active].text.is_empty());
        }
    }

    #[test]
    fn nested_numbering_handles_mixed_indentation_and_first_item_deletion() {
        let text = "1. parent\n\t1. child\n\t\t2. remaining\n    4. last\n\t2. sibling\n  \t1. new children\n\t\t8. next child\n2. parent\n\t3. new list";
        let mut normalized = text.to_owned();
        for (a, b, with) in numbered_edits(text).into_iter().rev() {
            normalized.replace_range(a..b, &with);
        }
        assert_eq!(
            normalized,
            "1. parent\n\t1. child\n\t\t1. remaining\n    2. last\n\t2. sibling\n  \t1. new children\n\t\t2. next child\n2. parent\n\t1. new list"
        );
        assert!(numbered_edits(&normalized).is_empty());
    }

    #[test]
    fn deleting_items_at_second_nested_level_renumbers_in_editor() {
        let text = "1. parent\n\t1. child\n\t\t1. first\n\t\t2. middle\n\t\t3. last";
        for deleted in ["\t\t1. first\n", "\t\t2. middle\n"] {
            let (mut app, ctx) = app_with(text);
            frame(&mut app, &ctx, vec![]);
            let a = text.find(deleted).unwrap();
            app.set_selection(&ctx, a, a + deleted.len());
            frame(&mut app, &ctx, vec![key(Key::Backspace, Modifiers::NONE)]);
            let expected = if deleted.contains("first") {
                "1. parent\n\t1. child\n\t\t1. middle\n\t\t2. last"
            } else {
                "1. parent\n\t1. child\n\t\t1. first\n\t\t2. last"
            };
            assert_eq!(app.docs[0].text, expected);
        }
    }

    #[test]
    fn numbering_tracks_siblings_and_preserves_code_and_starts() {
        let text = "3. a\n8. b\n  1. child\n  9. child\n7. c\n\nparagraph\n\n5) other\n9) next\n```md\n1. code\n9. code\n```\n";
        let mut normalized = text.to_owned();
        for (a, b, with) in numbered_edits(text).into_iter().rev() {
            normalized.replace_range(a..b, &with);
        }
        assert_eq!(
            normalized,
            "3. a\n4. b\n  1. child\n  2. child\n5. c\n\nparagraph\n\n5) other\n6) next\n```md\n1. code\n9. code\n```\n"
        );
        assert!(numbered_edits(&normalized).is_empty());
    }

    #[test]
    fn inserting_and_deleting_numbered_items_updates_following_numbers() {
        let (mut app, ctx) = app_with("1. one\n2. two\n3. three");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 6, 6);
        frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
        assert_eq!(app.docs[0].text, "1. one\n2. \n3. two\n4. three");
        app.set_selection(&ctx, 7, 11);
        frame(&mut app, &ctx, vec![key(Key::Backspace, Modifiers::NONE)]);
        assert_eq!(app.docs[0].text, "1. one\n2. two\n3. three");
        frame(&mut app, &ctx, vec![key(Key::Z, Modifiers::COMMAND)]);
        assert!(app.docs[0].text.contains("4. three"));
    }

    #[test]
    fn empty_bullet_height_and_continuation_indent_are_stable() {
        let (empty, _) = layout("- ", None, 400.0);
        let (filled, _) = layout("- text", None, 400.0);
        assert!((empty.rows[0].height() - filled.rows[0].height()).abs() < 1.0);
        let (g, _) = layout("- text\n  continuation\n  - child", None, 400.0);
        assert!((g.rows[1].pos.x - LOOK.indent()).abs() < 1.0);
        assert!(g.rows[2].pos.x > g.rows[0].pos.x);
    }

    #[test]
    fn create_and_delete_files() {
        let dir = std::env::temp_dir().join(format!("mdedit-new-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (mut app, _ctx) = app_with("");
        app.browser = Some(files::Browser::new(dir.clone()));
        app.new_file = Some("note".into());
        app.create_file();
        let path = dir.join("note.md");
        assert!(path.exists());
        assert_eq!(app.docs[0].path.as_deref(), Some(path.as_path()));
        app.delete(&path, false);
        assert!(!path.exists());
        assert!(app.docs.is_empty(), "the unchanged tab is closed");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pasted_image_is_embedded_as_jpeg() {
        let uri = jpeg_data_uri(image::RgbaImage::new(2400, 600)).unwrap();
        let data = base64::decode(uri.strip_prefix("data:image/jpeg;base64,").unwrap()).unwrap();
        let img = image::load_from_memory(&data).unwrap();
        assert_eq!((img.width(), img.height()), (1200, 300));
    }

    fn key(k: Key, modifiers: Modifiers) -> Event {
        Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers }
    }

    #[test]
    fn bold_toggles_selection() {
        let (mut app, ctx) = app_with("hello world");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 0, 5);
        frame(&mut app, &ctx, vec![key(Key::B, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "**hello** world");
        assert_eq!(app.selection(&ctx), Some((2, 7)));
        frame(&mut app, &ctx, vec![key(Key::B, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "hello world");
    }

    #[test]
    fn italic_inserts_pair_without_selection() {
        let (mut app, ctx) = app_with("ab");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 1, 1);
        frame(&mut app, &ctx, vec![key(Key::I, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "a**b");
        assert_eq!(app.selection(&ctx), Some((2, 2)));
    }

    #[test]
    fn line_formats() {
        let (mut app, ctx) = app_with("one\ntwo");
        frame(&mut app, &ctx, vec![]);
        app.set_selection(&ctx, 5, 5);
        frame(&mut app, &ctx, vec![key(Key::Num2, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "one\n## two");
        frame(&mut app, &ctx, vec![key(Key::Num2, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "one\ntwo");
        frame(&mut app, &ctx, vec![key(Key::L, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "one\n- [ ] two");
        frame(&mut app, &ctx, vec![key(Key::L, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "one\n- [x] two");
        frame(&mut app, &ctx, vec![key(Key::L, Modifiers::COMMAND)]);
        assert_eq!(app.docs[0].text, "one\ntwo");
        app.set_selection(&ctx, 0, 7);
        frame(&mut app, &ctx, vec![key(Key::Q, Modifiers::COMMAND | Modifiers::SHIFT)]);
        assert_eq!(app.docs[0].text, "> one\n> two");
    }

    #[test]
    fn find_and_replace() {
        let (mut app, ctx) = app_with("cat dog cat");
        frame(&mut app, &ctx, vec![key(Key::F, Modifiers::COMMAND)]);
        let f = app.find.as_mut().unwrap();
        f.query = "cat".into();
        f.replace = "cow".into();
        app.find_next(&ctx);
        assert_eq!(app.selection(&ctx), Some((0, 3)));
        app.replace_one(&ctx);
        assert_eq!(app.docs[0].text, "cow dog cat");
        assert_eq!(app.selection(&ctx), Some((8, 11)));
        app.replace_all(&ctx);
        assert_eq!(app.docs[0].text, "cow dog cow");
    }

    const LOOK: md::Look = md::Look { size: 16.0, line_height: 1.5, weight: 300, bold_weight: 600, code_weight: 400 };

    fn layout(text: &str, cursor: Option<usize>, wrap: f32) -> (Arc<Galley>, Vec<Deco>) {
        let ctx = Context::default();
        fonts::apply(&ctx, "", 200, 400, Lang::English);
        ctx.run_ui(RawInput::default(), |_| {}).textures_delta.clear();
        let mut layout = |job| ctx.fonts_mut(|f| f.layout_job(job));
        md::build(text, cursor, wrap, &LOOK, 1.0, &mut |_| None, &mut layout)
    }

    #[test]
    fn wrapped_list_items_keep_their_indent() {
        let text = format!("- {}", "word ".repeat(40));
        let (g, decos) = layout(&text, None, 300.0);
        assert!(g.rows.len() > 2);
        let x0 = g.pos_from_cursor(CCursor::new(2)).min.x;
        assert!((x0 - LOOK.indent()).abs() < 1.0, "first row text starts at the indent: {x0}");
        for row in &g.rows[1..] {
            assert!((row.pos.x - LOOK.indent()).abs() < 1.0, "wrapped row starts at the indent");
        }
        assert!(matches!(decos[0], Deco::Bullet(2)));
    }

    #[test]
    fn ctrl_scroll_zooms_editor_font() {
        let (mut app, ctx) = app_with("x");
        app.cfg.size = 16.0;
        frame(&mut app, &ctx, vec![Event::Zoom(1.25)]);
        assert_eq!(app.cfg.size, 20.0);
    }

    #[test]
    fn empty_lines_keep_their_height() {
        let (g, _) = layout("a\n\nb", None, 500.0);
        assert_eq!(g.rows.len(), 3);
        assert!(g.rows[1].size.y > 20.0, "blank line height: {}", g.rows[1].size.y);
    }

    #[test]
    fn ctrl_click_table_link_keeps_preview_and_source_cursor() {
        for alignment in ["---", ":---:", "---:"] {
            let text = format!("Outside\n\n| Link |\n| {alignment} |\n| [Zażółć](https://example.com) |\n");
            let (mut app, ctx) = app_with(&text);
            let render = |app: &mut App, modifiers: Modifiers, mut events: Vec<Event>| {
                let mut link = None;
                events.insert(0, Event::ModifiersChanged(modifiers));
                let input = RawInput {
                    events,
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| {
                    link = app.editor(ui).0;
                });
                (output, link)
            };
            let (mut output, _) = render(&mut app, Modifiers::NONE, vec![]);
            output.textures_delta.clear();
            app.set_selection(&ctx, 0, 0);
            let (mut output, _) = render(&mut app, Modifiers::NONE, vec![]);
            output.textures_delta.clear();
            let cache = app.docs[0].cache.as_ref().unwrap();
            let (cell, start) = cache
                .decos
                .iter()
                .find_map(|d| {
                    if let Deco::Table { rows, .. } = d
                        && let Deco::Link { start, .. } = &rows[1].links[0][0]
                    {
                        return Some((rows[1].cells[0].clone(), *start));
                    }
                    None
                })
                .unwrap();
            let pos = output
                .shapes
                .iter()
                .find_map(|s| {
                    if let Shape::Text(t) = &s.shape
                        && Arc::ptr_eq(&t.galley, &cell)
                    {
                        return Some(
                            t.pos + cell.pos_from_cursor(CCursor::new(start)).center().to_vec2() + vec2(2.0, 0.0),
                        );
                    }
                    None
                })
                .expect("painted table cell");
            for pressed in [true, false] {
                let (mut output, link) = render(
                    &mut app,
                    Modifiers::CTRL,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::CTRL,
                        },
                    ],
                );
                output.textures_delta.clear();
                assert_eq!(link.as_deref(), if pressed { None } else { Some("https://example.com") });
                assert_eq!(app.selection(&ctx), Some((0, 0)));
                assert!(app.docs[0].cache.as_ref().unwrap().decos.iter().any(|d| matches!(d, Deco::Table { .. })));
                assert_eq!(app.docs[0].text, text);
            }
            let (mut output, _) = render(&mut app, Modifiers::NONE, vec![]);
            output.textures_delta.clear();
            for pressed in [true, false] {
                let (mut output, _) = render(
                    &mut app,
                    Modifiers::NONE,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ],
                );
                output.textures_delta.clear();
            }
            let (mut output, _) = render(&mut app, Modifiers::NONE, vec![]);
            output.textures_delta.clear();
            assert!(!app.docs[0].cache.as_ref().unwrap().decos.iter().any(|d| matches!(d, Deco::Table { .. })));
        }
    }

    #[test]
    fn tables_are_detected_and_revealed_at_cursor() {
        let text = "| a | b |\n|:-:|---|\n| 1 | 2 |\nafter";
        let (_, decos) = layout(text, None, 500.0);
        let Some(Deco::Table { rows, align, .. }) = decos.first() else { panic!("no table") };
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].cells[1].text(), "2");
        assert_eq!(align[0], Align::Center);
        let (_, decos) = layout(text, Some(3), 500.0);
        assert!(!decos.iter().any(|d| matches!(d, Deco::Table { .. })), "raw source while editing");
    }

    /// Random mixes of Markdown syntax and multi-byte characters must lay out without panicking
    /// (the parser slices strings at byte offsets).
    #[test]
    fn layout_survives_random_markdown() {
        const TOKENS: &[&str] = &[
            "*", "**", "_", "`", "```", "~~~", "[", "]", "(", ")", "!", "#", "##", ">", "> ", "-", "- ", "- [ ] ",
            "1. ", "|", "|-", ":", "$", "$$", "\\", "\\[", "\\(", "~~", "==", "^", "%%", "<", "[^1]", "[^1]:", "[[",
            "]]", "![", "](", "http://x", "#tag", "[!note]", "---", "ą", "ż", "日本", "€", "🙂", "\n", "\n", " ", " ",
            "a", "word", "1", "\t",
        ];
        let ctx = Context::default();
        fonts::apply(&ctx, "", 300, 600, Lang::English);
        ctx.run_ui(RawInput::default(), |_| {}).textures_delta.clear();
        let mut layout = |job| ctx.fonts_mut(|f| f.layout_job(job));
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        // `FUZZ_ITERS=1000000 cargo test` runs it much longer.
        let iterations = std::env::var("FUZZ_ITERS").ok().and_then(|n| n.parse().ok()).unwrap_or(4000);
        for _ in 0..iterations {
            let text: String = (0..(next() % 40) + 1).map(|_| TOKENS[(next() as usize) % TOKENS.len()]).collect();
            let chars = text.chars().count();
            let cursor = (next() % 3 != 0).then(|| (next() as usize) % (chars + 1));
            let _ = md::build(&text, cursor, 300.0, &LOOK, 1.0, &mut |_| None, &mut layout);
        }
    }

    #[test]
    fn long_table_cells_wrap_and_rows_grow() {
        let long = "word ".repeat(40);
        let text = format!("| a | b |\n|---|---|\n| short | {long}|\n");
        let (g, decos) = layout(&text, None, 400.0);
        let Some(Deco::Table { rows, widths, .. }) = decos.first() else { panic!("no table") };
        assert!(widths.iter().sum::<f32>() <= 400.5, "columns fit the text width");
        assert!(rows[1].cells[1].rows.len() > 2, "the long cell wraps");
        let (head, body) = (rows[0].cells[0].size().y, rows[1].cells[1].size().y);
        assert!(body > head * 2.0, "the row grows with its content");
        // The galley reserves the row heights, so nothing overlaps the next paragraph.
        assert!(g.size().y >= head + body);
    }

    #[test]
    fn markers_hidden_except_on_cursor_line() {
        let text = "**a**\n**b**";
        let (galley, _) = layout(text, Some(8), 500.0);
        // Hidden markers take no room: the first row is just "a", the cursor line shows "**b**".
        let (hidden, shown) = (galley.rows[0].size.x, galley.rows[1].size.x);
        assert!(hidden < shown * 0.5, "markers on other lines are hidden ({hidden} vs {shown})");
    }

    #[test]
    fn pasted_image_is_scaled_webp() {
        let dir = std::env::temp_dir().join(format!("mdedit-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = save_webp(image::RgbaImage::new(2400, 600), &dir, "note").unwrap();
        assert!(name.ends_with(".webp"));
        let img = image::open(dir.join(&name)).unwrap();
        assert_eq!((img.width(), img.height()), (1200, 300));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
