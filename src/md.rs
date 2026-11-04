//! Markdown "live preview" layout: turns raw markdown into an egui `Galley`
//! where syntax markers are hidden (except on the cursor line) and text is styled.
//! Things a text layout cannot draw (bullets, checkboxes, rules, images, tables, formulas…) are
//! returned as `Deco`s and painted by the editor on top of the galley.
//! Colors and proportions follow Obsidian's default dark theme.

use std::sync::Arc;

use eframe::egui::epaint::text::VariationCoords;
use eframe::egui::{
    Align, Color32, FontFamily, FontId, Galley, Stroke, Vec2,
    text::{ByteIndex, CCursor, LayoutJob, LayoutSection, TextFormat},
};

use crate::latex;

/// User-adjustable text metrics.
#[derive(Clone, Copy, PartialEq)]
pub struct Look {
    pub size: f32,
    pub line_height: f32,
    /// CSS weights, used as the `wght` axis of variable fonts.
    pub weight: u16,
    pub bold_weight: u16,
    pub code_weight: u16,
}

impl Look {
    /// Width of one indentation step (lists, quotes).
    pub fn indent(&self) -> f32 {
        (self.size * 1.5).round()
    }
    pub fn table_row_height(&self) -> f32 {
        (self.size * self.line_height + 12.0).round()
    }
}

pub const BG: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1c);
pub const TEXT: Color32 = Color32::from_rgb(0xc8, 0xc8, 0xc8);
pub const HEAD: Color32 = TEXT;
pub const MUTED: Color32 = Color32::from_rgb(0xb3, 0xb3, 0xb3);
pub const DIM: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);
pub const ACCENT: Color32 = Color32::from_rgb(0x8a, 0x5c, 0xf5);
pub const LINK: Color32 = Color32::from_rgb(0xa6, 0x8a, 0xf9);
pub const CODE: Color32 = Color32::from_rgb(0xe6, 0xb4, 0x8a);
pub const CODE_BG: Color32 = Color32::from_rgb(0x23, 0x23, 0x23);
pub const BORDER: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
const MARK_BG: Color32 = Color32::from_rgb(0x55, 0x1d, 0x42);
pub const TAG: Color32 = Color32::from_rgb(0x38, 0xb3, 0x5c);

pub fn heading_size(level: u8, look: &Look) -> f32 {
    let scale = match level {
        1 => 1.618,
        2 => 1.462,
        3 => 1.318,
        4 => 1.188,
        5 => 1.076,
        _ => 1.0,
    };
    (look.size * scale).round()
}

/// Obsidian callout colors by type.
fn callout_color(kind: &str) -> Color32 {
    match kind.to_lowercase().as_str() {
        "tip" | "hint" | "important" | "success" | "check" | "done" => Color32::from_rgb(0x44, 0xcf, 0x6e),
        "warning" | "caution" | "attention" => Color32::from_rgb(0xe9, 0x97, 0x3f),
        "danger" | "error" | "failure" | "fail" | "missing" | "bug" => Color32::from_rgb(0xfb, 0x46, 0x4c),
        "question" | "help" | "faq" => Color32::from_rgb(0xe0, 0xde, 0x71),
        "example" => Color32::from_rgb(0xa8, 0x82, 0xff),
        "quote" | "cite" => Color32::from_rgb(0x9e, 0x9e, 0x9e),
        "abstract" | "summary" | "tldr" | "todo" => Color32::from_rgb(0x53, 0xdf, 0xdd),
        _ => Color32::from_rgb(0x02, 0x7a, 0xff),
    }
}

/// Visual extras, positions are char indices into the text.
pub enum Deco {
    Bullet(usize),
    Check {
        ci: usize,
        byte: usize,
        checked: bool,
    },
    Hr(usize),
    /// `color` is set for callouts (`> [!note]`).
    Quote {
        start: usize,
        end: usize,
        level: usize,
        color: Option<Color32>,
    },
    Code(usize, usize),
    /// `key` identifies the loaded image (computed once by the `img_size` callback).
    Image {
        ci: usize,
        key: String,
        size: Vec2,
    },
    Link {
        start: usize,
        end: usize,
        url: String,
    },
    /// The first row is the header; cells are laid out (wrapped) to the column widths.
    Table {
        rows: Vec<TableRow>,
        widths: Vec<f32>,
        align: Vec<Align>,
    },
    /// A rendered formula: centered on its own row(s) when `display`, else inline at `ci`.
    Math {
        ci: usize,
        galley: Arc<Galley>,
        boxed: bool,
        display: bool,
        /// egui puts `extra_letter_spacing` before each glyph, so the reserved room starts this much
        /// left of the first hidden character.
        shift: f32,
    },
}

pub struct TableRow {
    /// Char index of the source line, whose (hidden) row reserves the row's height.
    pub ci: usize,
    pub cells: Vec<Arc<Galley>>,
}

#[derive(Clone, Copy, PartialEq, Default)]
struct St {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    mark: bool,
    link: bool,
    tag: bool,
    /// Superscript (footnote references and labels).
    sup: bool,
    /// Slightly smaller text (footnote definitions).
    small: bool,
    hidden: bool,
    dim: bool,
    done: bool,
    mono: bool,
    block: bool,
    heading: u8,
    lh: u16,
    /// Extra advance of hidden characters (reserves room for inline formulas).
    spacing: f32,
    color: Option<Color32>,
}

fn fmt(s: St, look: &Look) -> TextFormat {
    if s.hidden {
        return TextFormat {
            font_id: FontId::proportional(0.1),
            color: Color32::TRANSPARENT,
            line_height: (s.lh > 0).then_some(s.lh as f32),
            extra_letter_spacing: s.spacing,
            ..Default::default()
        };
    }
    let mut size = heading_size(s.heading, look);
    if s.small {
        size = (size * 0.85).round();
    }
    if s.tag {
        size = (size * 0.9).round();
    }
    if s.sup {
        size = (size * 0.7).round();
    }
    let bold = s.bold || s.heading > 0;
    let mono = s.code || s.mono || s.block;
    let family = if mono {
        size = (size * 0.875).round();
        FontFamily::Monospace
    } else if bold && s.italic {
        FontFamily::Name("bi".into())
    } else if bold {
        FontFamily::Name("b".into())
    } else if s.italic {
        FontFamily::Name("i".into())
    } else {
        FontFamily::Name("ed".into())
    };
    let color = if s.dim || s.done {
        DIM
    } else if let Some(c) = s.color {
        c
    } else if s.tag {
        TAG
    } else if s.link {
        LINK
    } else if mono {
        CODE
    } else {
        TEXT
    };
    let background = if s.mark && !s.dim {
        MARK_BG
    } else if s.code && !s.dim {
        CODE_BG
    } else {
        Color32::TRANSPARENT
    };
    let lh = if s.heading > 0 { look.line_height.max(1.2) * 1.1 } else { look.line_height };
    let weight = if mono {
        look.code_weight
    } else if bold {
        look.bold_weight
    } else {
        look.weight
    };
    TextFormat {
        font_id: FontId::new(size, family),
        color,
        background,
        strikethrough: if s.strike || s.done { Stroke::new(1.0, color) } else { Stroke::NONE },
        line_height: Some(heading_size(s.heading, look) * lh),
        valign: if s.sup { Align::TOP } else { Align::Center },
        // Like browsers' `font-optical-sizing: auto` (Inter has an `opsz` axis from 14 to 32).
        coords: VariationCoords::new([(b"wght", weight as f32), (b"opsz", size.clamp(14.0, 32.0))]),
        ..Default::default()
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Normal,
    Fence,
    Code,
    TableRow,
    TableSep,
    Math,
    /// YAML front matter (`---` … `---` at the top of the file).
    Front,
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

fn is_table_sep(l: &str) -> bool {
    let t = l.trim();
    t.starts_with('|') && t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ' | '\t'))
}

fn table_cells(l: &str) -> Vec<String> {
    let t = l.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut cells = vec![String::new()];
    let mut esc = false;
    for c in t.chars() {
        match c {
            '|' if !esc => cells.push(String::new()),
            '\\' if !esc => esc = true,
            _ => {
                if esc && c != '|' {
                    cells.last_mut().unwrap().push('\\');
                }
                esc = false;
                cells.last_mut().unwrap().push(c);
            }
        }
    }
    cells.iter().map(|c| c.trim().to_owned()).collect()
}

const CELL_PAD: f32 = 12.0;

fn cell_galley(cx: &mut Cx, look: &Look, text: &str, wrap: f32, ppp: f32) -> Arc<Galley> {
    build(text, None, wrap, look, ppp, &mut |_| None, &mut *cx.layout).0
}

/// Column widths: natural widths when they fit, else the narrow columns keep theirs and the
/// wide ones share what is left equally.
fn column_widths(natural: &[f32], avail: f32) -> Vec<f32> {
    let mut widths = natural.to_vec();
    if natural.iter().sum::<f32>() <= avail {
        return widths;
    }
    let mut open: Vec<usize> = (0..natural.len()).collect();
    let mut left = avail;
    while !open.is_empty() {
        let share = left / open.len() as f32;
        let (fit, rest): (Vec<usize>, Vec<usize>) = open.iter().partition(|&&i| natural[i] <= share);
        if fit.is_empty() {
            for &i in &open {
                widths[i] = share.max(2.0 * CELL_PAD + 16.0);
            }
            break;
        }
        left -= fit.iter().map(|&i| natural[i]).sum::<f32>();
        open = rest;
    }
    widths
}

/// Lays out the table in lines `a..=b`; returns it with the reserved height of each source row.
fn layout_table(
    cx: &mut Cx,
    lines: &[&str],
    kinds: &[Kind],
    line_ci: &[usize],
    (a, b): (usize, usize),
    ppp: f32,
) -> (Deco, Vec<(usize, u16)>) {
    // Cell text is as small as tags.
    let small = Look { size: (cx.look.size * 0.9).round(), ..*cx.look };
    let align = table_cells(lines[a + 1])
        .iter()
        .map(|c| match (c.starts_with(':'), c.ends_with(':')) {
            (true, true) => Align::Center,
            (false, true) => Align::Max,
            _ => Align::Min,
        })
        .collect();
    let row_lines: Vec<usize> = (a..=b).filter(|&l| kinds[l] == Kind::TableRow).collect();
    let texts: Vec<Vec<String>> = row_lines
        .iter()
        .enumerate()
        .map(|(r, &l)| {
            let header = |c: String| if r == 0 && !c.is_empty() { format!("**{c}**") } else { c };
            table_cells(lines[l]).into_iter().map(header).collect()
        })
        .collect();
    let ncols = texts.iter().map(Vec::len).max().unwrap_or(0);
    let mut natural = vec![0.0f32; ncols];
    for row in &texts {
        for (c, text) in row.iter().enumerate() {
            let g = cell_galley(cx, &small, text, f32::INFINITY, ppp);
            natural[c] = natural[c].max(g.size().x + 2.0 * CELL_PAD);
        }
    }
    let widths = column_widths(&natural, cx.wrap);
    let min_height = cx.look.table_row_height();
    let mut heights = Vec::new();
    let rows = row_lines
        .iter()
        .zip(&texts)
        .map(|(&l, row)| {
            let cells: Vec<Arc<Galley>> = row
                .iter()
                .enumerate()
                .map(|(c, t)| cell_galley(cx, &small, t, widths[c] - 2.0 * CELL_PAD, ppp))
                .collect();
            let height = cells.iter().map(|g| g.size().y + 12.0).fold(min_height, f32::max).ceil();
            heights.push((l, height as u16));
            TableRow { ci: line_ci[l], cells }
        })
        .collect();
    (Deco::Table { rows, widths, align }, heights)
}

/// Display-math delimiters: (open, close).
const MATH_DELIMS: [(&str, &str); 2] = [("$$", "$$"), ("\\[", "\\]")];

/// The formula inside a display-math block spanning `lines`.
fn math_source(lines: &[&str]) -> String {
    let text: String = lines.concat();
    let t = text.trim();
    for (open, close) in MATH_DELIMS {
        if let Some(inner) = t.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
            return inner.to_owned();
        }
    }
    t.to_owned()
}

/// State shared by the line stylers.
struct Cx<'a> {
    look: &'a Look,
    wrap: f32,
    decos: Vec<Deco>,
    img_size: &'a mut dyn FnMut(&str) -> Option<(String, Vec2)>,
    layout: &'a mut dyn FnMut(LayoutJob) -> Arc<Galley>,
    /// Color of the callout the current quote block belongs to.
    callout: Option<Color32>,
}

impl Cx<'_> {
    /// Lays out a formula as plain text.
    fn math(&mut self, src: &str, size: f32, wrap: f32) -> (Arc<Galley>, bool) {
        let (text, boxed) = latex::to_unicode(src);
        let format = TextFormat {
            font_id: FontId::new(size, FontFamily::Name("i".into())),
            color: TEXT,
            coords: VariationCoords::new([(b"wght", self.look.weight as f32)]),
            ..Default::default()
        };
        let mut job = LayoutJob::single_section(text, format);
        job.wrap.max_width = wrap;
        ((self.layout)(job), boxed)
    }
}

/// Lays out `text` with egui's `layout` function. `cursor` is the char index of the text cursor
/// (its line shows raw markers). `img_size` returns the cache key and natural size of an image used by the document.
#[allow(clippy::too_many_arguments)]
pub fn build(
    text: &str,
    cursor: Option<usize>,
    wrap: f32,
    look: &Look,
    pixels_per_point: f32,
    img_size: &mut dyn FnMut(&str) -> Option<(String, Vec2)>,
    layout: &mut dyn FnMut(LayoutJob) -> Arc<Galley>,
) -> (Arc<Galley>, Vec<Deco>) {
    let mut job = LayoutJob { text: text.to_owned(), ..Default::default() };
    job.wrap.max_width = wrap;
    let mut cx = Cx { look, wrap, decos: Vec::new(), img_size, layout, callout: None };

    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let cur_line = cursor.map(|c| text.chars().take(c).filter(|&ch| ch == '\n').count());

    // Pre-pass: multi-line blocks (code, tables, formulas, front matter) as (first, last) line ranges.
    let mut kinds = vec![Kind::Normal; lines.len()];
    let mut blocks = Vec::new();
    let mut open: Option<(usize, &str)> = None;
    let mut i = 0;
    if lines.first().is_some_and(|l| l.trim_end() == "---")
        && let Some(end) = lines.iter().skip(1).position(|l| l.trim_end() == "---")
    {
        kinds[..=end + 1].fill(Kind::Front);
        blocks.push((0, end + 1));
        i = end + 2;
    }
    while i < lines.len() {
        let t = lines[i].trim_start();
        match open {
            Some((start, fence)) => {
                if t.trim_end().starts_with(fence) && t.trim_end().trim_start_matches(&fence[..1]).is_empty() {
                    kinds[i] = Kind::Fence;
                    blocks.push((start, i));
                    open = None;
                } else {
                    kinds[i] = Kind::Code;
                }
            }
            None if t.starts_with("```") || t.starts_with("~~~") => {
                kinds[i] = Kind::Fence;
                open = Some((i, &t[..3]));
            }
            None if t.starts_with('|') && lines.get(i + 1).is_some_and(|l| is_table_sep(l)) => {
                let start = i;
                kinds[i] = Kind::TableRow;
                kinds[i + 1] = Kind::TableSep;
                i += 2;
                while i < lines.len() && lines[i].trim_start().starts_with('|') {
                    kinds[i] = Kind::TableRow;
                    i += 1;
                }
                blocks.push((start, i - 1));
                continue;
            }
            None => {
                let math = MATH_DELIMS.iter().find(|(o, _)| t.starts_with(o)).and_then(|(o, c)| {
                    let tt = t.trim_end();
                    if tt.len() > o.len() + 1 && tt.ends_with(c) {
                        return Some(i);
                    }
                    (i + 1..lines.len()).find(|&j| lines[j].trim_end().ends_with(c))
                });
                if let Some(end) = math {
                    kinds[i..=end].fill(Kind::Math);
                    blocks.push((i, end));
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    if let Some((start, _)) = open {
        blocks.push((start, lines.len() - 1));
    }
    let block_of = |i: usize| blocks.iter().find(|(a, b)| (*a..=*b).contains(&i)).copied();
    let line_ci: Vec<usize> = lines
        .iter()
        .scan(0, |acc, l| {
            let start = *acc;
            *acc += chars(l);
            Some(start)
        })
        .collect();

    let mut galleys = Vec::with_capacity(lines.len());
    let mut table_heights: Vec<(usize, u16)> = Vec::new();
    let mut byte0 = 0;
    for (li, raw) in lines.iter().enumerate() {
        let ci0 = line_ci[li];
        let content = raw.strip_suffix('\n').unwrap_or(raw);
        let n = content.len();
        let rev = cur_line == Some(li);
        let mut st = vec![St::default(); raw.len()];
        let mut hang = None;
        match kinds[li] {
            Kind::Normal => hang = line(&mut cx, content, rev, &mut st, byte0, ci0),
            kind => {
                cx.callout = None;
                let (a, b) = block_of(li).unwrap_or((li, li));
                let block_rev = cur_line.is_some_and(|c| (a..=b).contains(&c));
                let delim = li == a || li == b;
                if li == a && kind == Kind::TableRow && !block_rev {
                    let (table, heights) = layout_table(&mut cx, &lines, &kinds, &line_ci, (a, b), pixels_per_point);
                    cx.decos.push(table);
                    table_heights = heights;
                }
                let s = match kind {
                    Kind::Fence | Kind::Front if delim && block_rev => {
                        St { dim: true, mono: true, ..Default::default() }
                    }
                    Kind::Fence => St { hidden: true, lh: 8, ..Default::default() },
                    Kind::Front if delim => St { hidden: true, lh: 8, ..Default::default() },
                    Kind::Front => St { block: true, color: Some(MUTED), ..Default::default() },
                    Kind::Code => St { block: true, ..Default::default() },
                    _ if block_rev => St { mono: true, ..Default::default() },
                    Kind::TableSep => St { hidden: true, lh: 1, ..Default::default() },
                    Kind::Math if li != a => St { hidden: true, lh: 1, ..Default::default() },
                    Kind::Math => {
                        let src = math_source(&lines[a..=b]);
                        let size = (look.size * 1.15).round();
                        let (galley, boxed) = cx.math(&src, size, wrap - 40.0);
                        let h = galley.size().y + if boxed { 36.0 } else { 20.0 };
                        cx.decos.push(Deco::Math { ci: ci0, galley, boxed, display: true, shift: 0.0 });
                        St { hidden: true, lh: h as u16, ..Default::default() }
                    }
                    _ => {
                        let lh = table_heights
                            .iter()
                            .find(|(l, _)| *l == li)
                            .map_or(look.table_row_height() as u16, |t| t.1);
                        St { hidden: true, lh, ..Default::default() }
                    }
                };
                st.fill(s);
                if kind == Kind::TableRow && !block_rev {
                    // Only the first hidden char reserves the row's height (a long line may wrap).
                    st.iter_mut().skip(1).for_each(|x| x.lh = 0);
                }
                if li == a && matches!(kind, Kind::Fence | Kind::Code | Kind::Front) {
                    let end_ci = line_ci[b] + chars(lines[b]);
                    cx.decos.push(Deco::Code(ci0, end_ci.saturating_sub(1)));
                }
            }
        }
        if raw.len() > n {
            let mut s = if n > 0 { st[n - 1] } else { st[n] };
            s.mark = false;
            s.code = false;
            s.tag = false;
            s.sup = false;
            s.spacing = 0.0;
            st[n] = s;
        }
        // The merged galley needs the sections too: egui reads text formats from them when painting.
        push_sections(&mut job, &st, byte0, look);

        // Each line is laid out as its own paragraph (like egui does internally), so that
        // wrapped rows of list items and quotes can be indented under their text.
        let last = li + 1 == lines.len();
        let plen = if last { raw.len() } else { n };
        let mut para = LayoutJob { text: raw[..plen].to_owned(), ..Default::default() };
        para.wrap.max_width = wrap - hang.map_or(0.0, |(x, _)| x);
        push_sections(&mut para, &st[..plen], 0, look);
        if plen == 0 {
            // An empty paragraph takes its height from the (zero-length) section format.
            let s = st.first().copied().unwrap_or_default();
            para.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(0)..ByteIndex(0),
                format: fmt(s, look),
            });
        }
        let g = (cx.layout)(para);
        galleys.push(match hang {
            Some((x, content_byte)) => {
                let mut g = (*g).clone();
                let cx = g.pos_from_cursor(CCursor::new(chars(&content[..content_byte]))).min.x;
                for (r, row) in g.rows.iter_mut().enumerate() {
                    row.pos.x += if r == 0 { (x - cx).max(0.0) } else { x };
                }
                Arc::new(g)
            }
            None => g,
        });
        byte0 += raw.len();
    }
    let decos = std::mem::take(&mut cx.decos);
    if galleys.is_empty() {
        return ((cx.layout)(job), decos);
    }
    (Arc::new(Galley::concat(Arc::new(job), &galleys, pixels_per_point)), decos)
}

fn push_sections(job: &mut LayoutJob, st: &[St], byte0: usize, look: &Look) {
    let mut start = 0;
    for i in 1..=st.len() {
        if i == st.len() || st[i] != st[start] {
            job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(byte0 + start)..ByteIndex(byte0 + i),
                format: fmt(st[start], look),
            });
            start = i;
        }
    }
}

fn is_hr(s: &str) -> bool {
    let t: Vec<u8> = s.bytes().filter(|b| *b != b' ').collect();
    t.len() >= 3 && matches!(t[0], b'-' | b'*' | b'_') && t.iter().all(|b| *b == t[0])
}

/// Styles one normal line. Returns the hanging indent (x, byte where the text starts)
/// for list items and quotes.
fn line(cx: &mut Cx, content: &str, rev: bool, st: &mut [St], byte0: usize, ci0: usize) -> Option<(f32, usize)> {
    let b = content.as_bytes();
    let n = b.len();
    let ci = |k: usize| ci0 + chars(&content[..k]);
    let hidden = St { hidden: true, ..St::default() };
    let mut base = St::default();
    let mut i = 0;
    let mut level = 0.0;

    let hashes = b.iter().take_while(|c| **c == b'#').count();
    if (1..=6).contains(&hashes) && (n == hashes || b[hashes] == b' ') {
        cx.callout = None;
        base.heading = hashes as u8;
        let m = (hashes + 1).min(n);
        st[..m].fill(marker(base, rev));
        i = m;
    } else if is_hr(content) {
        cx.callout = None;
        if rev {
            st[..n].fill(St { dim: true, ..base });
        } else {
            st[..n].fill(St { hidden: true, lh: 24, ..base });
            cx.decos.push(Deco::Hr(ci0));
        }
        return None;
    } else {
        let mut quotes = 0;
        while i < n && b[i] == b'>' {
            quotes += 1;
            i = if i + 1 < n && b[i + 1] == b' ' { i + 2 } else { i + 1 };
        }
        st[..i].fill(hidden);
        if quotes == 0 {
            cx.callout = None;
        } else {
            // Callout header: `> [!type] Title` (optionally `[!type]-` / `[!type]+` for folding).
            if content[i..].starts_with("[!")
                && let Some(close) = content[i..].find(']')
            {
                let kind = &content[i + 2..i + close];
                let color = callout_color(kind);
                cx.callout = Some(color);
                let mut end = i + close + 1;
                while end < n && matches!(b[end], b'-' | b'+' | b' ') {
                    end += 1;
                }
                let title = St { bold: true, color: Some(color), ..base };
                st[i..n].fill(title);
                if end < n {
                    st[i..end].fill(marker(title, rev));
                } else {
                    // No title: show the type name itself.
                    st[i..i + 2].fill(marker(title, rev));
                    st[i + close..n].fill(marker(title, rev));
                }
                cx.decos.push(Deco::Quote { start: ci0, end: ci0 + chars(content), level: quotes, color: cx.callout });
                return Some((quotes as f32 * cx.look.indent(), i));
            }
            cx.decos.push(Deco::Quote { start: ci0, end: ci0 + chars(content), level: quotes, color: cx.callout });
            level = quotes as f32;
        }
        let ws = &b[i..];
        let spaces = ws.iter().take_while(|c| **c == b' ' || **c == b'\t').count();
        let nest = ws[..spaces].iter().map(|c| if *c == b'\t' { 2 } else { 1 }).sum::<usize>() / 2;
        let j = i + spaces;
        if j < n && matches!(b[j], b'-' | b'*' | b'+') && (j + 1 == n || b[j + 1] == b' ') {
            let rest = &content[(j + 2).min(n)..];
            let task = ["[ ]", "[x]", "[X]"].iter().any(|t| rest == *t || rest.starts_with(&format!("{t} ")));
            let m = if task { (j + 6).min(n) } else { (j + 2).min(n) };
            st[i..m].fill(hidden);
            if task {
                let checked = rest.as_bytes()[1] != b' ';
                base.done = checked;
                cx.decos.push(Deco::Check { ci: ci(m), byte: byte0 + j + 3, checked });
            } else {
                cx.decos.push(Deco::Bullet(ci(m)));
            }
            i = m;
            level += nest as f32 + 1.0;
        } else {
            let d = b[j..].iter().take_while(|c| c.is_ascii_digit()).count();
            if d > 0 && j + d + 1 < n && matches!(b[j + d], b'.' | b')') && b[j + d + 1] == b' ' {
                st[i..j].fill(hidden);
                st[j..j + d + 2].fill(St { dim: true, ..base });
                i = j + d + 2;
                level += nest as f32 + 1.0;
            } else if content[j..].starts_with("[^")
                && let Some(close) = content[j..].find("]:")
            {
                // Footnote definition `[^1]: text`: a superscript label, then smaller text.
                base.small = true;
                let dim = St { dim: true, ..base };
                st[j..j + close + 2].fill(dim);
                st[j + 2..j + close].fill(St { sup: !rev, ..dim });
                st[j..j + 2].fill(marker(dim, rev));
                st[j + close..j + close + 2].fill(marker(dim, rev));
                let id = &content[j + 2..j + close];
                cx.decos.push(Deco::Link { start: ci(j), end: ci(j + close + 2), url: format!("^{id}:") });
                i = j + close + 2;
            }
        }
    }
    let hang = (level > 0.0).then_some((level * cx.look.indent(), i));
    st[i..n].fill(base);
    inline(cx, content, i, rev, st, ci0);
    hang
}

fn marker(s: St, rev: bool) -> St {
    if rev { St { dim: true, ..s } } else { St { hidden: true, lh: 0, ..s } }
}

/// Finds `[text](url)` starting at `i` (b[i] == '['); returns (index of ']', index of ')').
fn parse_link(b: &[u8], i: usize) -> Option<(usize, usize)> {
    let t = i + 1 + b[i + 1..].iter().position(|c| *c == b']')?;
    if b.get(t + 1) != Some(&b'(') {
        return None;
    }
    let from = if b.get(t + 2) == Some(&b'<') { t + 2 + b[t + 2..].iter().position(|c| *c == b'>')? } else { t + 2 };
    let c = from + b[from..].iter().position(|c| *c == b')')?;
    Some((t, c))
}

fn link_url(s: &str) -> String {
    let s = s.trim();
    if s.starts_with("data:") {
        return s.to_owned();
    }
    let s = match s.strip_prefix('<') {
        Some(r) => r.split('>').next().unwrap_or(r),
        None => s.split(' ').next().unwrap_or(s),
    };
    s.replace("%20", " ")
}

/// Splits Obsidian's size suffix: `alt|300` → ("alt", Some(300)).
fn image_width(alt: &str) -> Option<f32> {
    alt.rsplit_once('|').and_then(|(_, w)| w.trim().split('x').next()?.parse().ok())
}

/// Finds the end of inline math opened at `i`; returns (content start, content end, end of closing delimiter).
fn inline_math(s: &str, i: usize) -> Option<(usize, usize, usize)> {
    let b = s.as_bytes();
    let (open, close) = if s[i..].starts_with("\\(") {
        ("\\(", "\\)")
    } else if s[i..].starts_with("\\[") {
        ("\\[", "\\]")
    } else if s[i..].starts_with("$$") {
        ("$$", "$$")
    } else {
        ("$", "$")
    };
    let from = i + open.len();
    if open == "$" && b.get(from).is_none_or(|c| c.is_ascii_whitespace() || c.is_ascii_digit()) {
        return None;
    }
    let mut j = from + 1;
    while j + close.len() <= b.len() {
        // Compare bytes: `j` may sit inside a multi-byte character.
        if b[j..].starts_with(close.as_bytes()) && b[j - 1] != b'\\' {
            if open == "$" && (b[j - 1].is_ascii_whitespace() || b.get(j + 1).is_some_and(|c| c.is_ascii_digit())) {
                return None;
            }
            return Some((from, j, j + close.len()));
        }
        j += 1;
    }
    None
}

fn inline(cx: &mut Cx, s: &str, from: usize, rev: bool, st: &mut [St], ci0: usize) {
    let b = s.as_bytes();
    let n = b.len();
    let ci = |k: usize| ci0 + chars(&s[..k]);
    let mut used = vec![false; n];
    let mut i = from;
    let mark = |st: &mut [St], used: &mut [bool], r: std::ops::Range<usize>| {
        for k in r {
            st[k] = marker(st[k], rev);
            used[k] = true;
        }
    };
    while i < n {
        if used[i] {
            i += 1;
            continue;
        }
        let c = b[i];
        match c {
            b'\\' | b'$'
                if (c == b'$' || matches!(b.get(i + 1), Some(b'(' | b'[')))
                    && let Some((a, z, end)) = inline_math(s, i) =>
            {
                if rev {
                    for (k, x) in st.iter_mut().enumerate().take(end).skip(i) {
                        *x = St { dim: k < a || k >= z, mono: true, ..*x };
                    }
                } else {
                    let (galley, boxed) = cx.math(&s[a..z], cx.look.size, f32::INFINITY);
                    let w = galley.size().x + if boxed { 12.0 } else { 2.0 };
                    let spacing = w / chars(&s[i..end]) as f32;
                    st[i..end].fill(St { hidden: true, spacing, ..St::default() });
                    cx.decos.push(Deco::Math { ci: ci(i), galley, boxed, display: false, shift: spacing });
                }
                used[i..end].fill(true);
                i = end;
            }
            b'\\' if i + 1 < n && b[i + 1].is_ascii_punctuation() => {
                mark(st, &mut used, i..i + 1);
                used[i + 1] = true;
                i += 2;
            }
            b'`' => {
                let r = b[i..].iter().take_while(|x| **x == b'`').count();
                let close = (i + r..n.saturating_sub(r - 1)).find(|&j| {
                    b[j..j + r].iter().all(|x| *x == b'`') && b.get(j + r) != Some(&b'`') && b[j - 1] != b'`'
                });
                match close {
                    Some(j) => {
                        for k in i + r..j {
                            st[k].code = true;
                            used[k] = true;
                        }
                        mark(st, &mut used, i..i + r);
                        mark(st, &mut used, j..j + r);
                        i = j + r;
                    }
                    None => i += r,
                }
            }
            b'%' if s[i..].starts_with("%%") => {
                // Obsidian comment.
                let end = s[i + 2..].find("%%").map_or(n, |p| i + 2 + p + 2);
                for k in i..end {
                    st[k].dim = true;
                    used[k] = true;
                }
                i = end;
            }
            b'!' if b.get(i + 1) == Some(&b'[') => {
                // `![alt](src)` or Obsidian's `![[file.png|300]]`.
                let found = if b.get(i + 2) == Some(&b'[') {
                    s[i + 3..].find("]]").map(|p| {
                        let inner = &s[i + 3..i + 3 + p];
                        let src = inner.split('|').next().unwrap_or(inner).to_owned();
                        (src, image_width(inner), i + 3 + p + 1, None)
                    })
                } else {
                    parse_link(b, i + 1).map(|(t, cl)| {
                        let src = link_url(&s[t + 2..cl]);
                        let data = src.starts_with("data:").then_some(t + 2);
                        (src, image_width(&s[i + 2..t]), cl, data)
                    })
                };
                match found {
                    Some((src, width, cl, data)) => {
                        let loaded = (cx.img_size)(&src);
                        match loaded {
                            Some((key, sz)) if !rev => {
                                let w = width.unwrap_or(sz.x).min(sz.x).min(cx.wrap - 4.0).max(1.0);
                                let size = Vec2::new(w, sz.y * w / sz.x);
                                // Only the first hidden char reserves the image's height: a long `data:` URI
                                // wraps over many rows, and every row would otherwise get that height.
                                st[i..=cl].fill(St { hidden: true, ..St::default() });
                                st[i].lh = size.y as u16 + 12;
                                cx.decos.push(Deco::Image { ci: ci(i), key, size });
                            }
                            _ => {
                                st[i..=cl].iter_mut().for_each(|x| x.dim = true);
                                // Keep embedded base64 data out of sight even while editing the line.
                                if let Some(d) = data {
                                    let payload = s[d..cl].find(',').map_or(d, |p| d + p + 1);
                                    st[(payload + 24).min(cl)..=cl].fill(St { hidden: true, ..St::default() });
                                }
                            }
                        }
                        used[i..=cl].fill(true);
                        i = cl + 1;
                    }
                    None => i += 1,
                }
            }
            b'[' if b.get(i + 1) == Some(&b'^') => match s[i..].find(']') {
                // Footnote reference.
                Some(p) => {
                    for x in &mut st[i + 2..i + p] {
                        x.link = true;
                        x.sup = !rev;
                    }
                    cx.decos.push(Deco::Link {
                        start: ci(i),
                        end: ci(i + p + 1),
                        url: format!("^{}", &s[i + 2..i + p]),
                    });
                    mark(st, &mut used, i..i + 2);
                    mark(st, &mut used, i + p..i + p + 1);
                    used[i..=i + p].fill(true);
                    i += p + 1;
                }
                None => i += 1,
            },
            b'[' if b.get(i + 1) == Some(&b'[') => match s[i + 2..].find("]]") {
                Some(p) => {
                    let e = i + 2 + p;
                    st[i + 2..e].iter_mut().for_each(|x| x.link = true);
                    cx.decos.push(Deco::Link { start: ci(i), end: ci(e + 2), url: format!("[[{}", &s[i + 2..e]) });
                    mark(st, &mut used, i..i + 2);
                    mark(st, &mut used, e..e + 2);
                    i += 2;
                }
                None => i += 2,
            },
            b'[' => match parse_link(b, i) {
                Some((t, cl)) => {
                    st[i + 1..t].iter_mut().for_each(|x| x.link = true);
                    cx.decos.push(Deco::Link { start: ci(i), end: ci(cl + 1), url: link_url(&s[t + 2..cl]) });
                    mark(st, &mut used, i..i + 1);
                    mark(st, &mut used, t..cl + 1);
                    i += 1;
                }
                None => i += 1,
            },
            b'h' if (s[i..].starts_with("https://") || s[i..].starts_with("http://"))
                && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) =>
            {
                let e = i + s[i..].find(|ch: char| ch.is_whitespace() || ch == ')' || ch == '>').unwrap_or(n - i);
                for k in i..e {
                    st[k].link = true;
                    used[k] = true;
                }
                cx.decos.push(Deco::Link { start: ci(i), end: ci(e), url: s[i..e].to_owned() });
                i = e;
            }
            b'#' if (i == 0 || b[i - 1].is_ascii_whitespace()) => {
                // Tag: `#word`, `#nested/tag` (not only digits).
                let len = s[i + 1..]
                    .find(|ch: char| !(ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/')))
                    .unwrap_or(n - i - 1);
                let tag = &s[i + 1..i + 1 + len];
                if tag.chars().any(|ch| !ch.is_ascii_digit()) {
                    for k in i..i + 1 + len {
                        st[k].tag = true;
                        used[k] = true;
                    }
                }
                i += 1 + len;
            }
            b'*' | b'_' | b'~' | b'=' => {
                let r = b[i..].iter().take_while(|x| **x == c).count();
                let d = match c {
                    b'~' | b'=' if r >= 2 => 2,
                    b'~' | b'=' => {
                        i += r;
                        continue;
                    }
                    _ => r.min(3),
                };
                let opens = b.get(i + d).is_some_and(|x| !x.is_ascii_whitespace())
                    && (c != b'_' || i == 0 || !b[i - 1].is_ascii_alphanumeric());
                let close = opens
                    .then(|| {
                        (i + d + 1..n).find(|&j| {
                            !used[j]
                                && b[j] == c
                                && b[j - 1] != c
                                && !b[j - 1].is_ascii_whitespace()
                                && b[j..].iter().take_while(|x| **x == c).count() == d
                                && (c != b'_' || b.get(j + d).is_none_or(|x| !x.is_ascii_alphanumeric()))
                        })
                    })
                    .flatten();
                match close {
                    Some(j) => {
                        for x in &mut st[i + d..j] {
                            match (c, d) {
                                (b'~', _) => x.strike = true,
                                (b'=', _) => x.mark = true,
                                (_, 1) => x.italic = true,
                                (_, 2) => x.bold = true,
                                _ => {
                                    x.bold = true;
                                    x.italic = true;
                                }
                            }
                        }
                        mark(st, &mut used, i..i + d);
                        mark(st, &mut used, j..j + d);
                        i += d;
                    }
                    None => i += r,
                }
            }
            _ => i += 1,
        }
    }
}
