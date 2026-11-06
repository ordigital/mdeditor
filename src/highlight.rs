//! Syntax coloring for configuration files and shell scripts, chosen by the filename.
//! These files are shown as monospace text (no Markdown); colors come from the editor's palette.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::Arc;

use eframe::egui::epaint::text::VariationCoords;
use eframe::egui::{
    Align, Color32, FontFamily, FontId, Galley,
    text::{ByteIndex, LayoutJob, LayoutSection, TextFormat},
};

use crate::md::{self, Look};

mod bash;
mod engine;
mod worker;

pub use worker::{Analysis, Worker};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Syntax {
    Xml,
    Toml,
    Json,
    Yaml,
    Ini,
    Env,
    Bash,
    Html,
    Css,
    Php,
    Python,
    Rust,
    C,
    Cpp,
    Go,
    JavaScript,
}

struct Language {
    name: &'static str,
    syntax: Syntax,
    extensions: &'static [&'static str],
    filenames: &'static [&'static str],
    aliases: &'static [&'static str],
    engine: Option<&'static str>,
}

/// Detection, fenced aliases and grammar selection have one source of truth.
const LANGUAGES: &[Language] = &[
    Language {
        name: "XML",
        syntax: Syntax::Xml,
        extensions: &["xml"],
        filenames: &[],
        aliases: &["xml"],
        engine: None,
    },
    Language {
        name: "TOML",
        syntax: Syntax::Toml,
        extensions: &["toml"],
        filenames: &[],
        aliases: &["toml"],
        engine: None,
    },
    Language {
        name: "JSON",
        syntax: Syntax::Json,
        extensions: &["json", "jsonc"],
        filenames: &[],
        aliases: &["json", "jsonc"],
        engine: None,
    },
    Language {
        name: "YAML",
        syntax: Syntax::Yaml,
        extensions: &["yaml", "yml"],
        filenames: &[],
        aliases: &["yaml", "yml"],
        engine: None,
    },
    Language {
        name: "INI",
        syntax: Syntax::Ini,
        extensions: &["ini", "cfg", "conf"],
        filenames: &[],
        aliases: &["ini", "cfg", "conf"],
        engine: None,
    },
    Language {
        name: ".env",
        syntax: Syntax::Env,
        extensions: &["env"],
        filenames: &[".env"],
        aliases: &["env", "dotenv"],
        engine: None,
    },
    Language {
        name: "Bash",
        syntax: Syntax::Bash,
        extensions: &["sh", "bash"],
        filenames: &[".bashrc", ".bash_profile", ".bash_login", ".bash_logout", ".bash_aliases", ".profile"],
        aliases: &["sh", "bash", "shell"],
        engine: None,
    },
    Language {
        name: "HTML",
        syntax: Syntax::Html,
        extensions: &["html", "htm"],
        filenames: &[],
        aliases: &["html", "htm"],
        engine: Some("html"),
    },
    Language {
        name: "CSS",
        syntax: Syntax::Css,
        extensions: &["css"],
        filenames: &[],
        aliases: &["css"],
        engine: Some("css"),
    },
    Language {
        name: "PHP",
        syntax: Syntax::Php,
        extensions: &["php", "phtml", "php3", "php4", "php5", "php7", "php8"],
        filenames: &[],
        aliases: &["php"],
        engine: Some("php"),
    },
    Language {
        name: "Python",
        syntax: Syntax::Python,
        extensions: &["py", "pyw"],
        filenames: &[],
        aliases: &["python", "py"],
        engine: Some("py"),
    },
    Language {
        name: "Rust",
        syntax: Syntax::Rust,
        extensions: &["rs"],
        filenames: &[],
        aliases: &["rust", "rs"],
        engine: Some("rs"),
    },
    Language {
        name: "C",
        syntax: Syntax::C,
        extensions: &["c", "h"],
        filenames: &[],
        aliases: &["c"],
        engine: Some("c"),
    },
    Language {
        name: "C++",
        syntax: Syntax::Cpp,
        extensions: &["cpp", "cc", "cxx", "hpp", "hh", "hxx"],
        filenames: &[],
        aliases: &["cpp", "c++", "cxx"],
        engine: Some("cpp"),
    },
    Language {
        name: "Go",
        syntax: Syntax::Go,
        extensions: &["go"],
        filenames: &[],
        aliases: &["go", "golang"],
        engine: Some("go"),
    },
    Language {
        name: "JavaScript",
        syntax: Syntax::JavaScript,
        extensions: &["js", "mjs", "cjs"],
        filenames: &[],
        aliases: &["javascript", "js"],
        engine: Some("js"),
    },
];

impl Syntax {
    /// The menu uses the same registry as filename and fenced-language detection.
    pub fn available() -> impl Iterator<Item = Self> {
        LANGUAGES.iter().map(|language| language.syntax)
    }

    pub fn name(self) -> &'static str {
        self.language().name
    }

    fn language(self) -> &'static Language {
        LANGUAGES.iter().find(|l| l.syntax == self).expect("registered language")
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let name = path.file_name()?.to_string_lossy().to_ascii_lowercase();
        if name.starts_with(".env.") {
            return Some(Self::Env);
        }
        if let Some(language) = LANGUAGES.iter().find(|l| l.filenames.contains(&name.as_str())) {
            return Some(language.syntax);
        }
        let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
        LANGUAGES.iter().find(|l| l.extensions.contains(&extension.as_str())).map(|l| l.syntax)
    }

    pub fn from_alias(alias: &str) -> Option<Self> {
        let alias = alias.to_ascii_lowercase();
        LANGUAGES.iter().find(|l| l.aliases.contains(&alias.as_str())).map(|l| l.syntax)
    }
}

/// UTF-8 byte range and palette color; independent of fonts, wrapping and cursor position.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub range: std::ops::Range<usize>,
    pub color: Color32,
}

fn push_span(spans: &mut Vec<Span>, range: std::ops::Range<usize>, color: Color32) {
    if range.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut().filter(|last| last.range.end == range.start && last.color == color) {
        last.range.end = range.end;
    } else {
        spans.push(Span { range, color });
    }
}

fn native_spans(text: &str, syntax: Syntax) -> Vec<Span> {
    let classes = classify(text, syntax);
    let mut spans = Vec::new();
    let mut start = 0;
    for end in 1..=text.len() {
        if end == text.len() || (text.is_char_boundary(end) && classes[end] != classes[start]) {
            push_span(&mut spans, start..end, color(classes[start]));
            start = end;
        }
    }
    spans
}

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum Class {
    #[default]
    Text,
    Comment,
    Str,
    Key,
    Number,
    Keyword,
    Punct,
    Section,
    Tag,
    Attr,
}

const PINK: Color32 = Color32::from_rgb(0xfa, 0x99, 0xcd);

fn color(c: Class) -> Color32 {
    match c {
        Class::Text => md::TEXT,
        Class::Comment => md::DIM,
        Class::Str => md::TAG,
        Class::Key | Class::Attr => md::LINK,
        Class::Number | Class::Keyword => md::CODE,
        Class::Punct => md::MUTED,
        Class::Section | Class::Tag => PINK,
    }
}

fn paint(cls: &mut [Class], from: usize, to: usize, c: Class) {
    let to = to.min(cls.len());
    if from < to {
        cls[from..to].fill(c);
    }
}

/// Byte offsets of the lines of `text` (each with its end, without the newline).
fn lines(text: &str) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut start = 0;
    text.split_inclusive('\n').map(move |raw| {
        let s = start;
        start += raw.len();
        (s, s + raw.trim_end_matches('\n').trim_end_matches('\r').len())
    })
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.') || b >= 0x80
}

/// Colors a value in `b[from..to]`: strings, numbers, keywords, punctuation and a trailing comment.
/// `comments` are the bytes that start a comment (when at the start or after whitespace).
fn value(b: &[u8], from: usize, to: usize, comments: &[u8], cls: &mut [Class]) {
    let mut i = from;
    while i < to {
        let c = b[i];
        if comments.contains(&c) && (i == from || b[i - 1].is_ascii_whitespace()) {
            paint(cls, i, to, Class::Comment);
            return;
        }
        match c {
            b'"' | b'\'' => {
                let mut j = i + 1;
                while j < to && b[j] != c {
                    j += if b[j] == b'\\' && c == b'"' { 2 } else { 1 };
                }
                let end = (j + 1).min(to);
                paint(cls, i, end, Class::Str);
                i = end;
            }
            b'[' | b']' | b'{' | b'}' | b',' => {
                cls[i] = Class::Punct;
                i += 1;
            }
            _ if is_word(c) || matches!(c, b'+' | b'-') => {
                let mut j = i + 1;
                while j < to && is_word(b[j]) {
                    j += 1;
                }
                let token = &b[i..j];
                let t = std::str::from_utf8(token).unwrap_or("").to_ascii_lowercase();
                let numeric = token.first().is_some_and(|d| d.is_ascii_digit() || matches!(d, b'+' | b'-'))
                    && token.iter().any(u8::is_ascii_digit)
                    && token.iter().all(|d| d.is_ascii_alphanumeric() || matches!(d, b'_' | b'.' | b'+' | b'-'));
                let keyword = matches!(t.as_str(), "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "none");
                paint(
                    cls,
                    i,
                    j,
                    if numeric {
                        Class::Number
                    } else if keyword {
                        Class::Keyword
                    } else {
                        Class::Text
                    },
                );
                i = j;
            }
            _ => i += 1,
        }
    }
}

/// Position of the first `:` or `=` (as per `seps`) outside quotes in `b[from..to]` that ends a key.
fn key_end(b: &[u8], from: usize, to: usize, seps: &[u8], yaml: bool) -> Option<usize> {
    let mut i = from;
    while i < to {
        match b[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < to && b[i] != q {
                    i += 1;
                }
            }
            b'#' if yaml => return None,
            c if seps.contains(&c) && (!yaml || i + 1 >= to || b[i + 1] == b' ') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn json(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        match b[i] {
            b'"' => {
                let mut j = i + 1;
                while j < n && b[j] != b'"' && b[j] != b'\n' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(n);
                let mut k = end;
                while k < n && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                paint(cls, i, end, if b.get(k) == Some(&b':') { Class::Key } else { Class::Str });
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = text[i..].find('\n').map_or(n, |p| i + p);
                paint(cls, i, end, Class::Comment);
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(n, |p| i + 2 + p + 2);
                paint(cls, i, end, Class::Comment);
                i = end;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                cls[i] = Class::Punct;
                i += 1;
            }
            c if is_word(c) || c == b'-' => {
                let mut j = i + 1;
                while j < n && (is_word(b[j]) || b[j] == b'+' || b[j] == b'-') {
                    j += 1;
                }
                let t = &text[i..j];
                let number = t.bytes().next().is_some_and(|d| d.is_ascii_digit() || d == b'-')
                    && t.bytes().any(|d| d.is_ascii_digit());
                let class = if number {
                    Class::Number
                } else if matches!(t, "true" | "false" | "null") {
                    Class::Keyword
                } else {
                    Class::Text
                };
                paint(cls, i, j, class);
                i = j;
            }
            _ => i += 1,
        }
    }
}

fn yaml(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    for (start, end) in lines(text) {
        let mut i = start;
        while i < end && b[i] == b' ' {
            i += 1;
        }
        if i >= end {
            continue;
        }
        if b[i] == b'#' {
            paint(cls, i, end, Class::Comment);
            continue;
        }
        if matches!(&text[i..end], "---" | "...") {
            paint(cls, i, end, Class::Punct);
            continue;
        }
        while i < end && b[i] == b'-' && (i + 1 == end || b[i + 1] == b' ') {
            cls[i] = Class::Punct;
            i += 1;
            while i < end && b[i] == b' ' {
                i += 1;
            }
        }
        match key_end(b, i, end, b":", true) {
            Some(colon) => {
                paint(cls, i, colon, Class::Key);
                cls[colon] = Class::Punct;
                value(b, colon + 1, end, b"#", cls);
            }
            None => value(b, i, end, b"#", cls),
        }
    }
}

fn toml(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    // Closing delimiter of a multi-line string that is still open.
    let mut open: Option<&str> = None;
    for (start, end) in lines(text) {
        let mut from = start;
        if let Some(delim) = open {
            match text[start..end].find(delim) {
                Some(p) => {
                    from = start + p + 3;
                    paint(cls, start, from, Class::Str);
                    open = None;
                }
                None => {
                    paint(cls, start, end, Class::Str);
                    continue;
                }
            }
        } else {
            while from < end && b[from].is_ascii_whitespace() {
                from += 1;
            }
            if from >= end {
                continue;
            }
            if b[from] == b'#' {
                paint(cls, from, end, Class::Comment);
                continue;
            }
            if b[from] == b'[' {
                let close = text[from..end].rfind(']').map_or(end, |p| from + p + 1);
                paint(cls, from, close, Class::Section);
                value(b, close, end, b"#", cls);
                continue;
            }
            if let Some(eq) = key_end(b, from, end, b"=", false) {
                paint(cls, from, eq, Class::Key);
                cls[eq] = Class::Punct;
                from = eq + 1;
                while from < end && b[from] == b' ' {
                    from += 1;
                }
            }
        }
        // A value, possibly starting a multi-line string.
        let rest = &text[from..end];
        match ["\"\"\"", "'''"].into_iter().find(|d| rest.starts_with(d)) {
            Some(delim) => match rest[3..].find(delim) {
                Some(p) => {
                    paint(cls, from, from + 3 + p + 3, Class::Str);
                    value(b, from + 3 + p + 3, end, b"#", cls);
                }
                None => {
                    paint(cls, from, end, Class::Str);
                    open = Some(delim);
                }
            },
            None => value(b, from, end, b"#", cls),
        }
    }
}

fn ini(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    for (start, end) in lines(text) {
        let mut i = start;
        while i < end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            continue;
        }
        match b[i] {
            b';' | b'#' => paint(cls, i, end, Class::Comment),
            b'[' => {
                let close = text[i..end].find(']').map_or(end, |p| i + p + 1);
                paint(cls, i, close, Class::Section);
                value(b, close, end, b";#", cls);
            }
            _ => match key_end(b, i, end, b"=:", false) {
                Some(sep) => {
                    paint(cls, i, sep, Class::Key);
                    cls[sep] = Class::Punct;
                    value(b, sep + 1, end, b";#", cls);
                }
                None => value(b, i, end, b";#", cls),
            },
        }
    }
}

fn env(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    for (start, end) in lines(text) {
        let mut i = start;
        while i < end && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= end {
            continue;
        }
        if b[i] == b'#' {
            paint(cls, i, end, Class::Comment);
            continue;
        }
        if text[i..end].starts_with("export ") {
            paint(cls, i, i + 6, Class::Keyword);
            i += 7;
        }
        let Some(eq) = key_end(b, i, end, b"=", false) else {
            value(b, i, end, b"#", cls);
            continue;
        };
        paint(cls, i, eq, Class::Key);
        cls[eq] = Class::Punct;
        value(b, eq + 1, end, b"#", cls);
        // `$VAR` and `${VAR}` references.
        let mut j = eq + 1;
        while j < end {
            if b[j] == b'$' && cls[j] != Class::Comment {
                let stop = if b.get(j + 1) == Some(&b'{') {
                    text[j..end].find('}').map_or(end, |p| j + p + 1)
                } else {
                    (j + 1..end).find(|&k| !(b[k].is_ascii_alphanumeric() || b[k] == b'_')).unwrap_or(end)
                };
                paint(cls, j, stop, Class::Number);
                j = stop.max(j + 1);
            } else {
                j += 1;
            }
        }
    }
}

fn xml(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        if b[i] == b'&' {
            let end = text[i..].find([';', ' ', '<', '\n']).filter(|&p| b[i + p] == b';').map_or(i + 1, |p| i + p + 1);
            paint(cls, i, end, if end > i + 1 { Class::Number } else { Class::Text });
            i = end;
            continue;
        }
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &text[i..];
        let special =
            [("<!--", "-->", Class::Comment), ("<![CDATA[", "]]>", Class::Text), ("<?", "?>", Class::Section)]
                .into_iter()
                .find(|(open, _, _)| rest.starts_with(open));
        if let Some((open, close, class)) = special {
            let end = rest[open.len()..].find(close).map_or(n, |p| i + open.len() + p + close.len());
            paint(cls, i, end, class);
            i = end;
            continue;
        }
        if rest.starts_with("<!") {
            let end = rest.find('>').map_or(n, |p| i + p + 1);
            paint(cls, i, end, Class::Section);
            i = end;
            continue;
        }
        // A tag: `<name attr="value" …>` or `</name>`.
        let mut j = i + 1;
        if b.get(j) == Some(&b'/') {
            j += 1;
        }
        paint(cls, i, j, Class::Punct);
        let name_end = (j..n).find(|&k| b[k].is_ascii_whitespace() || matches!(b[k], b'>' | b'/')).unwrap_or(n);
        paint(cls, j, name_end, Class::Tag);
        j = name_end;
        while j < n && b[j] != b'>' {
            match b[j] {
                b'"' | b'\'' => {
                    let q = b[j];
                    let end = (j + 1..n).find(|&k| b[k] == q).map_or(n, |p| p + 1);
                    paint(cls, j, end, Class::Str);
                    j = end;
                }
                b'=' | b'/' => {
                    cls[j] = Class::Punct;
                    j += 1;
                }
                c if c.is_ascii_whitespace() => j += 1,
                _ => {
                    let end =
                        (j..n).find(|&k| b[k].is_ascii_whitespace() || matches!(b[k], b'=' | b'>' | b'/')).unwrap_or(n);
                    paint(cls, j, end.max(j + 1), Class::Attr);
                    j = end.max(j + 1);
                }
            }
        }
        if j < n {
            cls[j] = Class::Punct;
            j += 1;
        }
        i = j;
    }
}

fn classify(text: &str, syntax: Syntax) -> Vec<Class> {
    let mut cls = vec![Class::Text; text.len()];
    match syntax {
        Syntax::Json => json(text, &mut cls),
        Syntax::Yaml => yaml(text, &mut cls),
        Syntax::Toml => toml(text, &mut cls),
        Syntax::Ini => ini(text, &mut cls),
        Syntax::Env => env(text, &mut cls),
        Syntax::Xml => xml(text, &mut cls),
        Syntax::Bash => bash::classify(text, &mut cls),
        _ => {} // New languages are handled by the shared grammar engine.
    }
    cls
}

fn format_color(color: Color32, look: &Look) -> TextFormat {
    TextFormat {
        font_id: FontId::new((look.size * 0.875).round(), FontFamily::Monospace),
        color,
        line_height: Some(look.size * look.line_height),
        valign: Align::Center,
        coords: VariationCoords::new([(b"wght", f32::from(look.code_weight))]),
        ..Default::default()
    }
}

fn sections(text: &str, spans: &[Span], from: usize, to: usize, base: usize, look: &Look) -> Vec<LayoutSection> {
    let mut output = Vec::new();
    let mut offset = from;
    let start = spans.partition_point(|s| s.range.end <= from);
    let mut add = |a, b, color| {
        if a < b && text.is_char_boundary(a) && text.is_char_boundary(b) {
            output.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(a - base)..ByteIndex(b - base),
                format: format_color(color, look),
            });
        }
    };
    for span in spans[start..].iter().take_while(|s| s.range.start < to) {
        let a = span.range.start.max(from);
        let b = span.range.end.min(to);
        add(offset, a, md::TEXT);
        add(a, b, span.color);
        offset = b;
    }
    add(offset, to, md::TEXT);
    output
}

/// Lays out `text` as colored monospace lines (one galley per line, like the Markdown layout).
#[cfg(test)]
pub fn build(
    text: &str,
    syntax: Syntax,
    wrap: f32,
    look: &Look,
    pixels_per_point: f32,
    layout: &mut dyn FnMut(LayoutJob) -> Arc<Galley>,
) -> Arc<Galley> {
    let spans = native_spans(text, syntax);
    build_spans(text, &spans, wrap, look, pixels_per_point, layout, &mut LayoutCache::default())
}

/// Persistent line galleys survive egui's frame-local font cache eviction.
#[derive(Default)]
pub struct LayoutCache {
    fonts: u64,
    metrics: u64,
    lines: HashMap<u64, Arc<Galley>>,
}

impl LayoutCache {
    pub fn set_fonts(&mut self, fonts: u64) {
        if self.fonts != fonts {
            self.lines.clear();
            self.fonts = fonts;
        }
    }
}

pub fn build_spans(
    text: &str,
    spans: &[Span],
    wrap: f32,
    look: &Look,
    pixels_per_point: f32,
    layout: &mut dyn FnMut(LayoutJob) -> Arc<Galley>,
    cache: &mut LayoutCache,
) -> Arc<Galley> {
    let mut metrics = std::collections::hash_map::DefaultHasher::new();
    (
        wrap.to_bits(),
        pixels_per_point.to_bits(),
        look.size.to_bits(),
        look.line_height.to_bits(),
        look.weight,
        look.bold_weight,
        look.code_weight,
    )
        .hash(&mut metrics);
    let metrics = metrics.finish();
    if cache.metrics != metrics {
        cache.lines.clear();
        cache.metrics = metrics;
    }
    let mut next = HashMap::new();
    let mut job = LayoutJob { text: text.to_owned(), ..Default::default() };
    job.wrap.max_width = wrap;
    let mut galleys = Vec::new();
    for (start, end) in lines(text) {
        let raw_end = text[end..].find('\n').map_or(text.len(), |p| end + p + 1).max(end);
        job.sections.extend(sections(text, spans, start, raw_end, 0, look));
        let sections = sections(text, spans, start, end, start, look);
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        text[start..end].hash(&mut hash);
        for section in &sections {
            (&section.byte_range, section.format.color).hash(&mut hash);
        }
        let key = hash.finish();
        let galley = if let Some(galley) = next.get(&key).or_else(|| cache.lines.get(&key)) {
            Arc::clone(galley)
        } else {
            let mut para = LayoutJob { text: text[start..end].to_owned(), sections, ..Default::default() };
            para.wrap.max_width = wrap;
            if end == start {
                para.sections.push(LayoutSection {
                    leading_space: 0.0,
                    byte_range: ByteIndex(0)..ByteIndex(0),
                    format: format_color(md::TEXT, look),
                });
            }
            layout(para)
        };
        next.insert(key, galley.clone());
        galleys.push(galley);
    }
    cache.lines = next;
    if galleys.is_empty() {
        return layout(job);
    }
    Arc::new(Galley::concat(Arc::new(job), &galleys, pixels_per_point))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The class of the first occurrence of `needle` in `text`.
    fn class_of(text: &str, syntax: Syntax, needle: &str) -> Class {
        classify(text, syntax)[text.find(needle).unwrap()]
    }

    #[test]
    fn syntax_menu_uses_unique_named_registry_entries() {
        let syntaxes: Vec<_> = Syntax::available().collect();
        let mut unique = std::collections::HashSet::new();
        for syntax in syntaxes {
            assert!(unique.insert(syntax));
            assert!(!syntax.name().is_empty());
            assert!(syntax.language().aliases.iter().any(|alias| Syntax::from_alias(alias) == Some(syntax)));
        }
        assert_eq!(unique.len(), LANGUAGES.len());
    }

    #[test]
    fn syntax_follows_the_file_name() {
        let s = |p: &str| Syntax::from_path(Path::new(p));
        assert_eq!(s("a/b.JSON"), Some(Syntax::Json));
        assert_eq!(s("x.yml"), Some(Syntax::Yaml));
        assert_eq!(s("Cargo.toml"), Some(Syntax::Toml));
        assert_eq!(s("pom.xml"), Some(Syntax::Xml));
        assert_eq!(s("my.ini"), Some(Syntax::Ini));
        assert_eq!(s(".env"), Some(Syntax::Env));
        assert_eq!(s(".env.local"), Some(Syntax::Env));
        assert_eq!(s("prod.env"), Some(Syntax::Env));
        for name in [
            "script.sh",
            "script.BASH",
            ".bashrc",
            ".bash_profile",
            ".bash_login",
            ".bash_logout",
            ".bash_aliases",
            ".profile",
        ] {
            assert_eq!(s(name), Some(Syntax::Bash), "{name}");
        }
        assert_eq!(s("notes.md"), None);
        assert_eq!(s("README"), None);
    }

    #[test]
    fn colors_each_format() {
        let j = r#"{"name": "x", "n": 12, "ok": true}"#;
        assert!(class_of(j, Syntax::Json, "\"name\"") == Class::Key);
        assert!(class_of(j, Syntax::Json, "\"x\"") == Class::Str);
        assert!(class_of(j, Syntax::Json, "12") == Class::Number);
        assert!(class_of(j, Syntax::Json, "true") == Class::Keyword);

        let y = "# c\nname: demo\nlist:\n  - 3\n  - 'q'\n";
        assert!(class_of(y, Syntax::Yaml, "# c") == Class::Comment);
        assert!(class_of(y, Syntax::Yaml, "name") == Class::Key);
        assert!(class_of(y, Syntax::Yaml, "3") == Class::Number);
        assert!(class_of(y, Syntax::Yaml, "'q'") == Class::Str);

        let t = "[pkg]\nname = \"a\" # c\nn = 1\ntext = \"\"\"\nmulti\n\"\"\"\n";
        assert!(class_of(t, Syntax::Toml, "[pkg]") == Class::Section);
        assert!(class_of(t, Syntax::Toml, "name") == Class::Key);
        assert!(class_of(t, Syntax::Toml, "# c") == Class::Comment);
        assert!(class_of(t, Syntax::Toml, "multi") == Class::Str, "multi-line strings span lines");

        let i = "; c\n[s]\nkey=val\n";
        assert!(class_of(i, Syntax::Ini, "; c") == Class::Comment);
        assert!(class_of(i, Syntax::Ini, "[s]") == Class::Section);
        assert!(class_of(i, Syntax::Ini, "key") == Class::Key);

        let e = "export A=\"b\" # c\nB=${A}/x\n";
        assert!(class_of(e, Syntax::Env, "export") == Class::Keyword);
        assert!(class_of(e, Syntax::Env, "A=") == Class::Key);
        assert!(class_of(e, Syntax::Env, "\"b\"") == Class::Str);
        assert!(class_of(e, Syntax::Env, "${A}") == Class::Number);

        let x = "<!-- c --><a href=\"u\">t &amp; u</a>";
        assert!(class_of(x, Syntax::Xml, "<!--") == Class::Comment);
        assert!(class_of(x, Syntax::Xml, "a href") == Class::Tag);
        assert!(class_of(x, Syntax::Xml, "href") == Class::Attr);
        assert!(class_of(x, Syntax::Xml, "\"u\"") == Class::Str);
        assert!(class_of(x, Syntax::Xml, "&amp;") == Class::Number);
    }

    #[test]
    fn bash_colors_control_flow_assignments_parameters_and_comments() {
        let script = "#!/bin/bash\nexport NAME=world\nif [[ -n $NAME ]]; then\n  printf \"%s\\n\" \"${NAME:-guest}\" # comment\nfi\nCOUNT=42\necho word#literal\nHASH=#value\n";
        for keyword in ["export", "if", "then", "printf", "fi", "echo"] {
            assert_eq!(class_of(script, Syntax::Bash, keyword), Class::Keyword);
        }
        for parameter in ["$NAME", "${NAME:-guest}"] {
            assert_eq!(class_of(script, Syntax::Bash, parameter), Class::Number);
        }
        assert_eq!(class_of(script, Syntax::Bash, "NAME="), Class::Key);
        assert_eq!(class_of(script, Syntax::Bash, "42"), Class::Number);
        assert_eq!(class_of(script, Syntax::Bash, "#!"), Class::Comment);
        assert_eq!(class_of(script, Syntax::Bash, "# comment"), Class::Comment);
        assert_eq!(class_of(script, Syntax::Bash, "#literal"), Class::Text);
        assert_eq!(class_of(script, Syntax::Bash, "#value"), Class::Text);
        assert_eq!(class_of(script, Syntax::Bash, ";"), Class::Punct);
    }

    #[test]
    fn bash_respects_quotes_escapes_and_multiline_strings() {
        let script = r#"echo '$literal # text' "$expanded \$escaped # string" $'it\'s literal'
echo 'multi
# still string'
echo \#escaped foo\ #word
"#;
        for literal in ["$literal", "# text", "$escaped", "# string", "s literal", "# still string"] {
            assert_eq!(class_of(script, Syntax::Bash, literal), Class::Str, "{literal}");
        }
        assert_eq!(class_of(script, Syntax::Bash, "$expanded"), Class::Number);
        for literal in ["#escaped", "#word"] {
            assert_eq!(class_of(script, Syntax::Bash, literal), Class::Text);
        }
        for parameter in ["$?", "$1", "$@", "$$", "${x:-${fallback}}"] {
            let cls = classify(parameter, Syntax::Bash);
            assert!(cls.iter().all(|c| *c == Class::Number), "{parameter}");
        }
    }

    #[test]
    fn bash_heredocs_obey_quoted_delimiters_and_tab_stripping() {
        let script =
            "cat <<FIRST <<-'SECOND'\nhello $expanded # body\nFIRST\n\t$literal # body\n\tSECOND\nprintf done\n";
        assert_eq!(class_of(script, Syntax::Bash, "$expanded"), Class::Number);
        for literal in ["hello", "# body", "$literal"] {
            assert_eq!(class_of(script, Syntax::Bash, literal), Class::Str);
        }
        assert_eq!(class_of(script, Syntax::Bash, "printf"), Class::Keyword);
        assert_eq!(class_of(script, Syntax::Bash, "done"), Class::Keyword);
        let escaped = "cat <<\\EOF\n$plain\nEOF\necho ok";
        assert_eq!(class_of(escaped, Syntax::Bash, "$plain"), Class::Str);
        assert_eq!(class_of(escaped, Syntax::Bash, "echo"), Class::Keyword);
    }

    #[test]
    fn bash_shifts_and_here_strings_do_not_start_heredocs() {
        let script = "echo $((1 << 2))\nread <<<value\nprintf \"$after\"";
        assert_eq!(class_of(script, Syntax::Bash, "2"), Class::Number);
        assert_eq!(class_of(script, Syntax::Bash, "value"), Class::Text);
        assert_eq!(class_of(script, Syntax::Bash, "printf"), Class::Keyword);
        assert_eq!(class_of(script, Syntax::Bash, "$after"), Class::Number);
    }

    #[test]
    fn line_layout_cache_reuses_unchanged_lines_and_invalidates_fonts_and_wrap() {
        let ctx = eframe::egui::Context::default();
        crate::fonts::apply(&ctx, "", 300, 600, crate::i18n::Lang::English);
        ctx.run_ui(eframe::egui::RawInput::default(), |_| {}).textures_delta.clear();
        let look = Look { size: 16.0, line_height: 1.5, weight: 300, bold_weight: 600, code_weight: 400 };
        let calls = std::cell::Cell::new(0);
        let mut layout = |job| {
            calls.set(calls.get() + 1);
            ctx.fonts_mut(|f| f.layout_job(job))
        };
        let mut cache = LayoutCache::default();
        build_spans("one\ntwo", &[], 300.0, &look, 1.0, &mut layout, &mut cache);
        assert_eq!(calls.get(), 2);
        build_spans("one\ntwo", &[], 300.0, &look, 1.0, &mut layout, &mut cache);
        assert_eq!(calls.get(), 2);
        build_spans("one\nchanged", &[], 300.0, &look, 1.0, &mut layout, &mut cache);
        assert_eq!(calls.get(), 3);
        build_spans("one\nchanged", &[], 200.0, &look, 1.0, &mut layout, &mut cache);
        assert_eq!(calls.get(), 5);
        cache.set_fonts(1);
        build_spans("one\nchanged", &[], 200.0, &look, 1.0, &mut layout, &mut cache);
        assert_eq!(calls.get(), 7);
    }

    #[test]
    fn random_text_does_not_panic_and_lays_out() {
        const TOKENS: &[&str] = &[
            "\"", "'", "\"\"\"", "[", "]", "{", "}", ":", "=", "#", ";", "<", ">", "</", "/>", "<!--", "-->", "<?",
            "?>", "&", "${", "$", "-", "- ", "\\", "ą", "日本", "🙂", "\n", "\n", " ", "key", "12", "true", "export ",
            "<<", "<<<", "((", "))", "$?", "${x:-", "EOF", "fi", "echo",
        ];
        let ctx = eframe::egui::Context::default();
        crate::fonts::apply(&ctx, "", 300, 600, crate::i18n::Lang::English);
        ctx.run_ui(eframe::egui::RawInput::default(), |_| {}).textures_delta.clear();
        let look = Look { size: 16.0, line_height: 1.5, weight: 300, bold_weight: 600, code_weight: 400 };
        let mut layout = |job| ctx.fonts_mut(|f| f.layout_job(job));
        let mut seed = 0x1234_5678_9abc_def1u64;
        for _ in 0..3000 {
            let mut text = String::new();
            for _ in 0..(seed % 40) + 1 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                text += TOKENS[(seed as usize) % TOKENS.len()];
            }
            for syntax in
                [Syntax::Xml, Syntax::Toml, Syntax::Json, Syntax::Yaml, Syntax::Ini, Syntax::Env, Syntax::Bash]
            {
                let _ = build(&text, syntax, 300.0, &look, 1.0, &mut layout);
            }
        }
    }
}
