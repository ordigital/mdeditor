//! Font setup: the UI uses the desktop's font (KDE / GNOME settings resolved through fontconfig),
//! the editor uses a user-chosen family. Obsidian's defaults are embedded: Inter (text) and
//! Source Code Pro (code), both variable fonts whose weight is set per text run.

use std::process::Command;
use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

use crate::i18n::Lang;

/// Name shown for the embedded editor font (stored as an empty string in the config).
pub const BUILTIN: &str = "Inter (built-in)";

fn cmd(prog: &str, args: &[&str]) -> Option<String> {
    if cfg!(windows) {
        return None; // fontconfig and the desktop settings tools do not exist there
    }
    let out = Command::new(prog).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The desktop UI font as (family, size in points); Noto Sans 10 when it cannot be determined.
pub fn system_ui_font() -> (String, f32) {
    let parse = |family: &str, size: &str| {
        let size = size.trim().parse::<f32>().ok().filter(|s| *s > 4.0)?;
        Some((family.trim().to_owned(), size))
    };
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let kde = config.and_then(|c| std::fs::read_to_string(c.join("kdeglobals")).ok()).and_then(|s| {
        let v = s.lines().find_map(|l| l.strip_prefix("font="))?.to_owned();
        let mut parts = v.split(',');
        parse(parts.next()?, parts.next()?)
    });
    let gnome = || {
        let v = cmd("gsettings", &["get", "org.gnome.desktop.interface", "font-name"])?;
        let (family, size) = v.trim_matches('\'').rsplit_once(' ')?;
        parse(family, size)
    };
    kde.or_else(gnome).unwrap_or_else(|| ("Noto Sans".to_owned(), 10.0))
}

/// All font families known to fontconfig, sorted.
pub fn families() -> Vec<String> {
    let mut v: Vec<String> = cmd("fc-list", &[":", "family"])
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split(',').next().map(|s| s.trim().to_owned()))
        .filter(|s| !s.is_empty())
        .collect();
    v.sort_by_key(|s| s.to_lowercase());
    v.dedup();
    v
}

/// CSS weight (100–900) to fontconfig's weight scale.
#[allow(clippy::match_overlapping_arm)] // arms are ordered thresholds
fn fc_weight(css: u16) -> u16 {
    match css {
        ..=100 => 0,
        ..=200 => 40,
        ..=300 => 50,
        ..=400 => 80,
        ..=500 => 100,
        ..=600 => 180,
        ..=700 => 200,
        ..=800 => 205,
        _ => 210,
    }
}

/// Font file for `family`, optionally with a CSS weight and italic.
/// For italic, returns None when fontconfig only has an upright face at that weight.
/// A system font that has the glyphs of `lang`'s script, if the language needs one.
fn script_fallback(lang: Lang) -> Option<FontData> {
    if cfg!(windows) {
        let candidates: &[&str] = match lang {
            Lang::Chinese => &[r"C:\Windows\Fonts\msyh.ttc"],
            Lang::Japanese => &[r"C:\Windows\Fonts\meiryo.ttc"],
            Lang::Arabic | Lang::Urdu | Lang::Hebrew => &[r"C:\Windows\Fonts\arial.ttf"],
            Lang::Hindi | Lang::Bengali => &[r"C:\Windows\Fonts\Nirmala.ttf"],
            _ => &[],
        };
        return candidates.iter().find_map(|path| std::fs::read(path).ok().map(FontData::from_owned));
    }
    let tag = match lang {
        Lang::Chinese => "zh-cn",
        Lang::Japanese => "ja",
        Lang::Arabic | Lang::Urdu => "ar",
        Lang::Hebrew => "he",
        Lang::Hindi => "hi",
        Lang::Bengali => "bn",
        _ => return None,
    };
    let out = cmd("fc-match", &["-f", "%{file}|%{index}", &format!(":lang={tag}")])?;
    let (file, index) = out.split_once('|')?;
    let mut data = FontData::from_owned(std::fs::read(file).ok()?);
    data.index = index.parse().unwrap_or(0);
    Some(data)
}

fn fc_match(family: &str, weight: Option<u16>, italic: bool) -> Option<FontData> {
    let mut pattern: String =
        family.chars().flat_map(|c| if "-:,\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
    if let Some(w) = weight {
        pattern += &format!(":weight={}", fc_weight(w));
    }
    if italic {
        pattern += ":slant=100";
    }
    let out = cmd("fc-match", &["-f", "%{file}|%{index}|%{slant}", &pattern])?;
    let mut parts = out.split('|');
    let (file, index, slant) = (parts.next()?, parts.next()?, parts.next()?);
    if italic && slant == "0" {
        return None;
    }
    let ext = file.rsplit('.').next()?.to_lowercase();
    if !["ttf", "otf", "ttc", "otc"].contains(&ext.as_str()) {
        return None;
    }
    let mut data = FontData::from_owned(std::fs::read(file).ok()?);
    data.index = index.parse().unwrap_or(0);
    Some(data)
}

/// Like `fc_match`, but for italic falls back to the nearest heavier weight that has an italic face.
fn fc_styled(family: &str, weight: u16, italic: bool) -> Option<FontData> {
    if !italic {
        return fc_match(family, Some(weight), false);
    }
    (weight..=900).step_by(100).find_map(|w| fc_match(family, Some(w), true))
}

/// Installs the fonts. `editor` is a fontconfig family or "" for the embedded Inter;
/// `weight` and `bold` are CSS weights of normal and bold editor text.
pub fn apply(ctx: &Context, editor: &str, weight: u16, bold: u16, lang: Lang) {
    let mut fonts = FontDefinitions::default();
    let emoji = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut add = |name: &str, data: FontData| {
        fonts.font_data.insert(name.to_owned(), Arc::new(data));
    };
    add("inter", FontData::from_static(include_bytes!("../assets/InterVariable.ttf")));
    add("inter-i", FontData::from_static(include_bytes!("../assets/InterVariable-Italic.ttf")));
    add("code", FontData::from_static(include_bytes!("../assets/SourceCodeVF-Upright.ttf")));

    let (ui_family, ui_pt) = system_ui_font();
    let ui = fc_match(&ui_family, None, false).map(|d| add("ui", d)).map(|_| "ui");
    let mut ed = |w: u16, italic: bool, name: &'static str| {
        (!editor.is_empty()).then(|| fc_styled(editor, w, italic)).flatten().map(|d| add(name, d)).map(|_| name)
    };
    let editor_fonts =
        [ed(weight, false, "ed"), ed(bold, false, "ed-b"), ed(weight, true, "ed-i"), ed(bold, true, "ed-bi")];

    let fallbacks = ["inter", "inter", "inter-i", "inter-i"];

    let chain = |first: Option<&str>, fallback: &str| {
        first.into_iter().chain([fallback]).map(str::to_owned).chain(emoji.iter().cloned()).collect::<Vec<_>>()
    };
    // Glyphs the built-in fonts lack (CJK, Arabic, Hebrew, Indic scripts) come from a system font.
    let script = script_fallback(lang).map(|d| add("script", d)).map(|_| "script");
    let proportional = ui.into_iter().chain(["inter"]).chain(script).map(str::to_owned);
    fonts.families.insert(FontFamily::Proportional, proportional.chain(emoji.iter().cloned()).collect());
    fonts.families.insert(FontFamily::Monospace, chain(None, "code"));
    for ((name, fallback), font) in ["ed", "b", "i", "bi"].into_iter().zip(fallbacks).zip(editor_fonts) {
        fonts.families.insert(FontFamily::Name(name.into()), chain(font, fallback));
    }
    ctx.set_fonts(fonts);

    // Points to logical pixels (96 dpi).
    let px = (ui_pt * 96.0 / 72.0).round();
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Small, FontId::proportional((px * 0.85).round())),
            (TextStyle::Body, FontId::proportional(px)),
            (TextStyle::Button, FontId::proportional(px)),
            (TextStyle::Heading, FontId::proportional((px * 1.35).round())),
            (TextStyle::Monospace, FontId::monospace(px)),
        ]
        .into();
    });
}
