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
use crate::md::{self, Deco};
use eframe::egui::epaint::FontColorTransferFunction;

const MAX_LINE_W: f32 = 720.0;
const MAX_IMG_W: u32 = 1200;
/// Undo steps kept per document and entries in File → Recent.
const HISTORY: usize = 10;
const RECENT: usize = 10;
const TITLE_SCALE: f32 = 1.618;
/// Obsidian's `--background-secondary` (menu, tabs, dialogs) and a lighter status bar.
const CHROME: Color32 = Color32::from_rgb(0x26, 0x26, 0x26);
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
    PastePrimary,
    Exit,
    OpenRecent(usize),
    Fmt(Fmt),
}

struct Cache {
    key: u64,
    line: Option<usize>,
    galley: Arc<Galley>,
    decos: Vec<Deco>,
}

struct Doc {
    id: u64,
    path: Option<PathBuf>,
    text: String,
    saved: String,
    cache: Option<Cache>,
    /// Text as of the last recorded change, and the undo/redo snapshots.
    prev: String,
    undo: Vec<String>,
    redo: Vec<String>,
    last_edit: f64,
    /// Config files (JSON, YAML, …) are shown as colored plain text instead of Markdown.
    syntax: Option<Syntax>,
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
            path,
            saved: text.clone(),
            prev: text.clone(),
            text,
            cache: None,
            undo: vec![],
            redo: vec![],
            last_edit: f64::NEG_INFINITY,
            crlf: false,
            stats: Default::default(),
        }
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
    optimize: bool,
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
            optimize: true,
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
                "optimize" => self.optimize = on,
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
            "lang={}\noptimize={}\nline_numbers={}\ninline_title={}\nfiles={}\nembed_images={}\nfont={}\nweight={}\nbold_weight={}\ncode_weight={}\ngamma={}\nsize={}\nline_height={}\n",
            self.lang.code(),
            b(self.optimize),
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
    close_queue: Vec<u64>,
    quit_after: bool,
    allow_quit: bool,
    rx: Receiver<Vec<PathBuf>>,
    clipboard: Option<arboard::Clipboard>,
    v_pressed: bool,
    pasted: bool,
    scroll_to: Option<usize>,
    /// Editor scroll offset to apply on the next frame (drag-selection auto-scroll).
    scroll_pending: Option<f32>,
    title: String,
    recent: Vec<PathBuf>,
    browser: Option<files::Browser>,
    /// Name being typed in the "Create new" dialog of the file panel.
    new_file: Option<String>,
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
    s + k.symbol_or_name()
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
        by(t("Preferences…"), &[Action::Preferences], ""),
        by(t("About"), &[Action::About], ""),
        by(t("Bold"), &[f(Fmt::Bold)], "**text**"),
        by(t("Italic"), &[f(Fmt::Italic)], "*text*"),
        by(t("Strikethrough"), &[f(Fmt::Strike)], "~~text~~"),
        by(t("Inline code"), &[f(Fmt::Code)], "`code`"),
        by(t("Highlight"), &[f(Fmt::Mark)], "==text=="),
        by(t("Link"), &[f(Fmt::Link)], "[text](url)"),
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
        (save_all_mods(), Key::S, SaveAll),
        (cs, Key::S, SaveAs),
        (c, Key::S, Save),
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
    let p = Path::new(src);
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
            close_queue: vec![],
            quit_after: false,
            allow_quit: false,
            rx,
            clipboard: arboard::Clipboard::new().ok(),
            v_pressed: false,
            pasted: false,
            scroll_to: None,
            scroll_pending: None,
            title: String::new(),
            recent: Config::load_recent(),
            browser: None,
            new_file: None,
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
        if let Some(i) = self.docs.iter().position(|d| d.path.as_deref() == Some(&p)) {
            self.active = i;
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

    fn save(&mut self, i: usize, ask: bool) -> bool {
        let doc = &mut self.docs[i];
        if ask || doc.path.is_none() {
            let name = format!("{}.md", doc.stem(self.cfg.lang));
            let mut dlg = rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown"]).set_file_name(name);
            if let Some(d) = doc.dir() {
                dlg = dlg.set_directory(d);
            }
            let Some(mut p) = dlg.save_file() else {
                return false;
            };
            if p.extension().is_none() {
                p.set_extension("md");
            }
            doc.syntax = Syntax::from_path(&p);
            doc.path = Some(p.clone());
            self.add_recent(&p);
        }
        let doc = &mut self.docs[i];
        let path = doc.path.clone().unwrap();
        let data = if doc.crlf { doc.text.replace('\n', "\r\n") } else { doc.text.clone() };
        match std::fs::write(&path, data) {
            Ok(()) => {
                doc.saved = doc.text.clone();
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
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            return;
        }
        self.new_file = None;
        let mut path = browser.dir.join(name);
        if path.extension().is_none() {
            path.set_extension("md");
        }
        browser.reload();
        let created = std::fs::OpenOptions::new().write(true).create_new(true).open(&path);
        match created {
            Ok(_) => self.open_path(path),
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
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
            let marker_width = rest.saturating_sub(prefix.len()).max(next.len());
            self.insert(ctx, &format!("\n{prefix}{}", " ".repeat(marker_width)));
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
            let n = if line.starts_with('\t') { 1 } else { line.bytes().take(2).take_while(|c| *c == b' ').count() };
            (ls..ls + n, String::new(), a.saturating_sub(n).max(lsc))
        } else {
            return;
        };
        self.checkpoint(ctx);
        self.docs[self.active].text.replace_range(range, &with);
        self.set_selection(ctx, cursor, cursor);
        self.checkpoint(ctx);
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
            Action::New => self.new_doc(),
            Action::Open => {
                let mut dlg = rfd::FileDialog::new().add_filter("Markdown", &["md", "markdown", "txt"]);
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
            Action::Insert => self.insert_image_file(ctx),
            Action::Preferences => self.settings = true,
            Action::About => self.about = true,
            Action::Undo => self.undo_redo(ctx, false),
            Action::Redo => self.undo_redo(ctx, true),
            Action::Files => self.cfg.files = !self.cfg.files,
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
            // Markdown formatting makes no sense in a config file.
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
                ui.separator();
                item(ui, acts, t("Preferences…"), Action::Preferences);
            });
            ui.menu_button(t("View"), |ui| {
                ui.checkbox(&mut self.cfg.optimize, t("Optimize"));
                ui.checkbox(&mut self.cfg.line_numbers, t("Line numbers"));
                ui.checkbox(&mut self.cfg.inline_title, t("Inline title"));
                ui.checkbox(&mut self.cfg.files, t("Files")).on_hover_text(label(Action::Files));
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

        let Self { docs, active, images, cfg, find, scroll_to, scroll_pending, .. } = self;
        let doc = &mut docs[*active];
        let id = doc.edit_id();
        let dir = doc.dir();
        let syntax = doc.syntax;
        let gutter = if cfg.line_numbers { 48.0 } else { 0.0 };
        let full = ui.available_width();
        let col_w = if cfg.optimize { (full - gutter - 48.0).min(MAX_LINE_W) } else { full - gutter - 48.0 }.max(100.0);
        let left = if cfg.optimize { ((full - col_w) / 2.0).max(gutter + 16.0) } else { gutter + 24.0 };
        let mut clicked_link = None;
        let mut middle = None;
        let now = self.now;
        let look = cfg.look();
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
                    let mut img_size = |src: &str| {
                        let key = image_key(dir.as_deref(), src)?;
                        let entry = images.entry(key.clone()).or_insert_with(|| load_image(&ctx, dir.as_deref(), src));
                        entry.as_ref().map(|(_, size)| (key, *size))
                    };
                    let mut layouter = |ui: &Ui, buf: &dyn TextBuffer, wrap: f32| {
                        let text = buf.as_str();
                        let line = cursor_ci.map(|c| text.chars().take(c).filter(|ch| *ch == '\n').count());
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        let l = &look;
                        (text, line, wrap.to_bits(), ppp.to_bits(), l.size.to_bits(), l.line_height.to_bits())
                            .hash(&mut h);
                        (l.weight, l.bold_weight, l.code_weight, &font, syntax).hash(&mut h);
                        let key = h.finish();
                        if let Some(c) = cache.as_ref().filter(|c| c.key == key) {
                            return c.galley.clone();
                        }
                        let mut layout = |job| ui.ctx().fonts_mut(|f| f.layout_job(job));
                        let (galley, decos) = match syntax {
                            Some(syntax) => (highlight::build(text, syntax, wrap, &look, ppp, &mut layout), vec![]),
                            None => md::build(text, cursor_ci, wrap, &look, ppp, &mut img_size, &mut layout),
                        };
                        *cache = Some(Cache { key, line, galley: galley.clone(), decos });
                        galley
                    };
                    let min_h = (ui.available_height() - 20.0).max(200.0);
                    let out = TextEdit::multiline(&mut doc.text)
                        .id(id)
                        .frame(egui::Frame::NONE)
                        .margin(Margin::ZERO)
                        .desired_width(col_w)
                        .min_size(vec2(col_w, min_h))
                        .lock_focus(true)
                        .layouter(&mut layouter)
                        .show(ui);
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
                    let new_line = out
                        .cursor_range
                        .map(|r| doc.text.chars().take(r.primary.index.0).filter(|ch| *ch == '\n').count());
                    if new_line.is_some() && new_line != cache.line {
                        ctx.request_repaint();
                    }

                    let g = &out.galley;
                    let o = out.galley_pos.to_vec2();
                    let gx = out.galley_pos.x;
                    let at = |ci: usize| g.pos_from_cursor(CCursor::new(ci)).translate(o);
                    let painter = ui.painter();
                    let pointer = ctx.input(|i| i.pointer.interact_pos());
                    let clicked = out.response.clicked();
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
            let r = dialog(ctx, "about_modal", &format!("{} MDEdit", t("About")), 460.0, |ui| {
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
                centered_row(ui, |ui| ui.button("OK").clicked() || enter)
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
    for d in decos {
        match d {
            Deco::Code(a, b) => {
                let r = Rect::from_min_max(
                    pos2(cx.pos.x - 10.0, at(*a).min.y - 2.0),
                    pos2(cx.pos.x + cx.col_w + 10.0, at(*b).max.y + 2.0),
                );
                bgs.push(Shape::rect_filled(r, 4.0, md::CODE_BG));
            }
            Deco::Quote { start, end, level, color } => {
                let (r1, r2) = (at(*start), at(*end));
                for k in 0..*level {
                    let x = cx.pos.x + k as f32 * cx.look.indent() + 1.0;
                    let r = Rect::from_min_max(pos2(x, r1.min.y), pos2(x + 2.0, r2.max.y));
                    cx.painter.rect_filled(r, 0.0, color.unwrap_or(md::ACCENT));
                }
                if let Some(c) = color {
                    let r = Rect::from_min_max(pos2(cx.pos.x, r1.min.y), pos2(cx.pos.x + cx.col_w, r2.max.y));
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
            Deco::Table { rows, widths, align } => paint_table(cx.painter, cx.pos.x, rows, widths, align, &at),
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
                if cx.clicked
                    && cx.ctx.input(|i| i.modifiers.command)
                    && let Some(p) = cx.pointer
                {
                    let c = cx.galley.cursor_from_pos(p - cx.pos).index.0;
                    if (*start..*end).contains(&c) {
                        events.link = Some(url.clone());
                    }
                }
            }
        }
    }
    events
}

/// Draws a markdown table over the hidden source lines (each line reserved its row's height).
fn paint_table(
    painter: &egui::Painter,
    x0: f32,
    rows: &[md::TableRow],
    widths: &[f32],
    align: &[Align],
    at: &dyn Fn(usize) -> Rect,
) {
    const PAD: f32 = 12.0;
    let right = x0 + widths.iter().sum::<f32>();
    let stroke = Stroke::new(1.0, md::BORDER);
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return;
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
                painter.galley(pos2(gx, cell.center().y - g.size().y / 2.0), g.clone(), md::TEXT);
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
    let b = line.as_bytes();
    let mut i = b.iter().take_while(|c| **c == b' ' || **c == b'\t').count();
    let mut quoted = false;
    while b.get(i) == Some(&b'>') {
        quoted = true;
        i += if b.get(i + 1) == Some(&b' ') { 2 } else { 1 };
    }
    let ws = b[i..].iter().take_while(|c| **c == b' ' || **c == b'\t').count();
    let prefix = line[..i + ws].to_owned();
    let rest = &line[i + ws..];
    for task in ["- [ ] ", "- [x] ", "- [X] "] {
        if rest.starts_with(task) {
            return Some((prefix, "- [ ] ".into(), i + ws + task.len()));
        }
    }
    for bullet in ["- ", "* ", "+ "] {
        if rest.starts_with(bullet) {
            return Some((prefix, bullet.into(), i + ws + 2));
        }
    }
    let d = rest.bytes().take_while(|c| c.is_ascii_digit()).count();
    if d > 0 && matches!(rest.as_bytes().get(d), Some(b'.' | b')')) && rest.as_bytes().get(d + 1) == Some(&b' ') {
        let n: u64 = rest[..d].parse().ok()?;
        return Some((prefix, format!("{}{} ", n.saturating_add(1), &rest[d..d + 1]), i + ws + d + 2));
    }
    quoted.then(|| (line[..i].to_owned(), String::new(), i))
}

/// Byte replacements for sibling numbering; child lists keep independent counters.
fn numbered_edits(text: &str) -> Vec<(usize, usize, String)> {
    let mut edits = vec![];
    let mut stack: Vec<(String, char, u64)> = vec![];
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let mut front = text.starts_with("---\n");
    for (li, raw) in text.split_inclusive('\n').enumerate() {
        let line = raw.trim_end_matches(['\n', '\r']);
        let t = line.trim_start();
        let run = t.bytes().take_while(|b| Some(b) == t.as_bytes().first()).count();
        if front {
            if li > 0 && matches!(t, "---" | "...") {
                front = false;
            }
            offset += raw.len();
            continue;
        }
        if let Some((ch, count)) = fence {
            if t.as_bytes().first() == Some(&ch) && run >= count && t[run..].trim().is_empty() {
                fence = None;
            }
            offset += raw.len();
            continue;
        }
        if run >= 3 && matches!(t.as_bytes().first(), Some(b'`' | b'~')) {
            fence = Some((t.as_bytes()[0], run));
            stack.clear();
        } else if let Some((prefix, next, _rest)) = list_prefix(line) {
            if !next.is_empty() {
                // Match visual indentation, including mixed tabs/spaces produced by Tab.
                let key = prefix.replace('\t', "  ");
                while stack.last().is_some_and(|(p, _, _)| p != &key && !key.starts_with(p.as_str())) {
                    stack.pop();
                }
                let body = &line[prefix.len()..];
                let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                if digits > 0 {
                    let delim = body.as_bytes()[digits] as char;
                    let number = body[..digits].parse::<u64>().unwrap_or(1);
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
    let strip_task = |s: &str| -> Option<(bool, String)> {
        for (p, c) in [("- [ ] ", false), ("- [x] ", true), ("- [X] ", true)] {
            if let Some(r) = s.strip_prefix(p) {
                return Some((c, r.to_owned()));
            }
        }
        None
    };
    match f {
        Fmt::Heading(n) => {
            let hashes = body.bytes().take_while(|b| *b == b'#').count();
            let level = if hashes > 0 && body[hashes..].starts_with(' ') { hashes } else { 0 };
            let rest = if level > 0 { &body[level + 1..] } else { body };
            if n == 0 || level == n as usize { rest.to_owned() } else { format!("{} {rest}", "#".repeat(n as usize)) }
        }
        Fmt::Bullet => match strip_task(body) {
            Some((_, r)) => format!("{indent}- {r}"),
            None => match body.strip_prefix("- ") {
                Some(r) => format!("{indent}{r}"),
                None => format!("{indent}- {body}"),
            },
        },
        Fmt::Task => match strip_task(body) {
            Some((false, r)) => format!("{indent}- [x] {r}"),
            Some((true, r)) => format!("{indent}{r}"),
            None => format!("{indent}- [ ] {}", body.strip_prefix("- ").unwrap_or(body)),
        },
        Fmt::Quote => match l.strip_prefix("> ") {
            Some(r) => r.to_owned(),
            None => format!("> {l}"),
        },
        _ => l.to_owned(),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.now = ctx.input(|i| i.time);

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

        let modal_open = !self.close_queue.is_empty()
            || self.error.is_some()
            || self.about
            || self.settings
            || self.new_file.is_some()
            || self.delete_file.is_some();
        let mut acts = vec![];
        if !modal_open {
            for &(m, k, a) in shortcuts() {
                if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k))) {
                    acts.push(a);
                }
            }
        }
        for a in acts.drain(..) {
            self.run(&ctx, a);
        }
        if !modal_open {
            self.list_keys(&ctx);
        }
        self.detect_image_paste(&ctx);
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
            Some(d) => format!("{} — MDEdit", d.title(self.cfg.lang)),
            None => "MDEdit".to_owned(),
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
    fn about_lists_every_shortcut() {
        let covered: Vec<Action> = about_rows(Lang::English).into_iter().flat_map(|r| r.covers).collect();
        for &(m, k, a) in shortcuts() {
            assert!(covered.contains(&a), "About is missing {} ({})", shortcut_label(m, k), k.name());
        }
    }
    use eframe::App as _;
    use egui::{Event, RawInput};

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
            assert_eq!(app.docs[0].text, format!("{text}\n{}", " ".repeat(marker.len())));
            frame(&mut app, &ctx, vec![Event::Text("ciąg dalszy".into())]);
            frame(&mut app, &ctx, vec![key(Key::Enter, Modifiers::NONE)]);
            assert!(app.docs[0].text.ends_with(if marker == "- " { "\n- " } else { "\n2. " }));
        }
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
