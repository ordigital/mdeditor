//! Color emoji use one embedded Noto font for palette, editor and PDF overlays.
//! Keep ordinary Unicode text and existing egui cursor/layout metrics; decode the
//! font's PNG glyphs with dependencies already used by images and PDF export.
use eframe::egui::{self, Color32, Context, Galley, Painter, Pos2, Rect, TextureHandle, Vec2};
use std::sync::{Arc, OnceLock};

fn face() -> &'static ttf_parser::Face<'static> {
    static FACE: OnceLock<ttf_parser::Face<'static>> = OnceLock::new();
    FACE.get_or_init(|| {
        ttf_parser::Face::parse(include_bytes!("../assets/NotoColorEmoji.ttf"), 0)
            .expect("bundled Noto Color Emoji font")
    })
}

pub fn png(ch: char) -> Option<&'static [u8]> {
    if !matches!(ch as u32, 0x2300..=0x23ff | 0x2600..=0x27bf | 0x1f000..=0x1f0ff | 0x1f300..=0x1faff) {
        return None;
    }
    let face = face();
    let glyph = face.glyph_raster_image(face.glyph_index(ch)?, u16::MAX)?;
    (glyph.format == ttf_parser::RasterImageFormat::PNG).then_some(glyph.data)
}

pub fn texture(ctx: &Context, ch: char) -> Option<TextureHandle> {
    let id = egui::Id::new(("color_emoji", ch));
    if let Some(texture) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return Some(texture);
    }
    let image = image::load_from_memory(png(ch)?).ok()?.into_rgba8();
    let image =
        egui::ColorImage::from_rgba_unmultiplied([image.width() as usize, image.height() as usize], image.as_raw());
    let texture = ctx.load_texture(format!("emoji-{:x}", ch as u32), image, Default::default());
    ctx.data_mut(|data| data.insert_temp(id, texture.clone()));
    Some(texture)
}

pub fn paint_character(ctx: &Context, painter: &Painter, ch: char, rect: Rect) {
    if !painter.clip_rect().intersects(rect) {
        return;
    }
    if let Some(texture) = texture(ctx, ch) {
        let size = texture.size_vec2();
        let size = size * (rect.width() / size.x).min(rect.height() / size.y);
        painter.image(
            texture.id(),
            Rect::from_center_size(rect.center(), size),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

/// Suppress only monochrome glyph quads, preserving text, selection backgrounds,
/// source indices, wrapping and undo. Copy-on-write leaves egui's font cache intact.
pub fn layout(mut galley: Arc<Galley>) -> Arc<Galley> {
    if !galley.rows.iter().any(|row| row.glyphs.iter().any(|g| g.font_height > 2.0 && png(g.chr).is_some())) {
        return galley;
    }
    for placed in &mut Arc::make_mut(&mut galley).rows {
        if !placed.glyphs.iter().any(|g| g.font_height > 2.0 && png(g.chr).is_some()) {
            continue;
        }
        let row = Arc::make_mut(&mut placed.row);
        for glyph in &row.glyphs {
            if glyph.font_height > 2.0 && png(glyph.chr).is_some() {
                let start = glyph.first_vertex as usize;
                if let Some(vertices) = row.visuals.mesh.vertices.get_mut(start..start.saturating_add(4)) {
                    let position = vertices[0].pos;
                    for vertex in vertices {
                        vertex.color = Color32::TRANSPARENT;
                        // Selection and disabled text may override vertex colors.
                        // A degenerate quad stays invisible under those overrides.
                        vertex.pos = position;
                    }
                }
            }
        }
    }
    galley
}

pub fn paint(ctx: &Context, painter: &Painter, galley: &Galley, pos: Pos2) {
    for row in &galley.rows {
        if !painter.clip_rect().intersects(row.rect().translate(pos.to_vec2())) {
            continue;
        }
        for glyph in &row.glyphs {
            if glyph.font_height > 2.0 && png(glyph.chr).is_some() {
                let rect = glyph.logical_rect().translate(pos.to_vec2() + row.pos.to_vec2());
                let size = glyph.advance_width.min(glyph.font_height);
                paint_character(ctx, painter, glyph.chr, Rect::from_center_size(rect.center(), Vec2::splat(size)));
            }
        }
    }
}

pub fn characters() -> &'static [char] {
    static CHARACTERS: OnceLock<Vec<char>> = OnceLock::new();
    CHARACTERS.get_or_init(|| {
        let fonts = eframe::egui::FontDefinitions::default();
        let mut chars = std::collections::BTreeSet::new();
        for name in &fonts.families[&eframe::egui::FontFamily::Proportional] {
            let data = &fonts.font_data[name];
            let Ok(face) = ttf_parser::Face::parse(&data.font, data.index) else { continue };
            let Some(cmap) = face.tables().cmap else { continue };
            for table in cmap.subtables.into_iter().filter(|t| t.is_unicode()) {
                table.codepoints(|code| {
                    // Emoji/symbol blocks, excluding standalone skin-tone modifiers.
                    if matches!(code, 0x2300..=0x23ff | 0x2600..=0x27bf | 0x1f000..=0x1f0ff | 0x1f300..=0x1faff)
                        && !matches!(code, 0x1f3fb..=0x1f3ff)
                        && face.glyph_index(char::from_u32(code).unwrap()).is_some_and(|g| g.0 != 0)
                        && png(char::from_u32(code).unwrap()).is_some()
                    {
                        chars.insert(char::from_u32(code).unwrap());
                    }
                });
            }
        }
        let mut chars: Vec<_> = chars.into_iter().collect();
        // Start with faces, then the remaining symbols in stable Unicode order.
        chars.sort_by_key(|c| (!matches!(*c as u32, 0x1f600..=0x1f64f), *c));
        chars
    })
}

struct Annotation {
    name: &'static str,
    terms: &'static str,
}

fn annotations() -> &'static std::collections::HashMap<(char, &'static str), Annotation> {
    use std::io::Read;
    static TEXT: OnceLock<String> = OnceLock::new();
    static ANNOTATIONS: OnceLock<std::collections::HashMap<(char, &'static str), Annotation>> = OnceLock::new();
    ANNOTATIONS.get_or_init(|| {
        let text = TEXT.get_or_init(|| {
            let mut text = String::new();
            flate2::read::GzDecoder::new(include_bytes!("../assets/emoji-search.tsv.gz").as_slice())
                .read_to_string(&mut text)
                .expect("bundled CLDR emoji annotations");
            text
        });
        text.lines()
            .map(|line| {
                let mut fields = line.splitn(4, '\t');
                let ch = fields.next().unwrap().chars().next().unwrap();
                let language = fields.next().unwrap();
                ((ch, language), Annotation { name: fields.next().unwrap(), terms: fields.next().unwrap() })
            })
            .collect()
    })
}

pub fn name(ch: char, language: crate::i18n::Lang) -> &'static str {
    let entries = annotations();
    entries.get(&(ch, language.code())).or_else(|| entries.get(&(ch, "en"))).map_or("", |a| a.name)
}

/// Match every query word against localized or English CLDR terms, or the emoji
/// itself. The generated index also includes accent-free keyword spellings.
pub fn search(query: &str, language: crate::i18n::Lang) -> Vec<char> {
    let query = query.replace('\u{fe0f}', "").to_lowercase();
    let words: Vec<_> = query.split_whitespace().collect();
    if words.is_empty() {
        return characters().to_vec();
    }
    let entries = annotations();
    characters()
        .iter()
        .copied()
        .filter(|ch| {
            let local = entries.get(&(*ch, language.code())).map_or("", |a| a.terms);
            let english = entries.get(&(*ch, "en")).map_or("", |a| a.terms);
            words.iter().all(|word| local.contains(word) || english.contains(word) || ch.to_string() == *word)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn search_matches_localized_keywords_english_and_emoji() {
        use crate::i18n::Lang;
        assert_eq!(super::search("  ", Lang::Polish), super::characters());
        assert!(super::search("SeRcE", Lang::Polish).contains(&'❤'));
        assert!(super::search("czerwone serce", Lang::Polish).contains(&'❤'));
        assert!(super::search("zolw", Lang::Polish).contains(&'🐢'));
        assert!(super::search("ŻÓŁW", Lang::Polish).contains(&'🐢'));
        assert_eq!(super::search("❤️", Lang::Polish), ['❤']);
        assert!(super::search("no_such_emoji_92329", Lang::Polish).is_empty());
        for language in Lang::ALL {
            assert!(super::search("heart", language).contains(&'❤'));
            assert!(!super::name('😀', language).is_empty(), "{language:?}");
            assert!(super::annotations().contains_key(&('❤', language.code())), "{language:?}");
        }
    }

    #[test]
    fn color_glyphs_preserve_text_and_font_cache() {
        use eframe::egui::{self, Color32, FontId};
        let rgba = image::load_from_memory(super::png('😀').unwrap()).unwrap().into_rgba8();
        assert!(rgba.pixels().any(|p| p[0] as u16 > p[2] as u16 + 40 && p[1] as u16 > p[2] as u16 + 40));
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            let original =
                ui.fonts_mut(|fonts| fonts.layout_no_wrap("a😀b".into(), FontId::proportional(20.0), Color32::WHITE));
            let before = original.clone();
            let colored = super::layout(original.clone());
            assert_eq!(original, before);
            assert_eq!(colored.job.text, original.job.text);
            assert_eq!(colored.size(), original.size());
            let row = &colored.rows[0];
            let glyph = row.glyphs.iter().find(|g| g.chr == '😀').unwrap();
            let start = glyph.first_vertex as usize;
            assert!(row.visuals.mesh.vertices[start..start + 4].iter().all(|v| v.color.a() == 0));
            assert!(
                row.visuals.mesh.vertices[start..start + 4]
                    .iter()
                    .all(|v| v.pos == row.visuals.mesh.vertices[start].pos)
            );
            assert!(original.rows[0].visuals.mesh.vertices[start..start + 4].iter().any(|v| v.color.a() > 0));
            super::paint(ui.ctx(), ui.painter(), &colored, ui.min_rect().min);
            let a = super::texture(ui.ctx(), '😀').unwrap();
            assert_eq!(a.id(), super::texture(ui.ctx(), '😀').unwrap().id());
        });
        output.textures_delta.clear();
    }

    #[test]
    fn palette_uses_bundled_glyphs_and_contains_common_emoji() {
        let chars = super::characters();
        assert!(chars.len() > 100);
        for ch in ['😀', '❤', '☕', '👍'] {
            assert!(chars.contains(&ch), "{ch}");
        }
        assert_eq!(chars.iter().collect::<std::collections::HashSet<_>>().len(), chars.len());
        let ctx = eframe::egui::Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            for ch in chars {
                assert!(
                    ui.fonts_mut(|f| f.glyph_width(&eframe::egui::FontId::proportional(22.0), *ch)) > 0.0,
                    "{ch} U+{:X}",
                    *ch as u32
                );
            }
        });
        output.textures_delta.clear();
    }
}
