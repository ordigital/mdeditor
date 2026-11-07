//! One PDF backend for export and native printing; editor layout stays in md/app.
use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Context, FontDefinitions, Shape, TextFormat, TextureId};
use krilla::{
    Document,
    color::rgb,
    geom::{PathBuilder, Point, Rect, Size, Transform},
    page::PageSettings,
    paint::Fill,
    surface::Surface,
    text::{Font, GlyphId, KrillaGlyph, Tag},
};

pub const WIDTH: f32 = 595.28;
pub const HEIGHT: f32 = 841.89;
pub const MARGIN: f32 = 42.0;
pub const COLUMN: f32 = WIDTH - 2.0 * MARGIN;
pub const CONTENT_HEIGHT: f32 = HEIGHT - 2.0 * MARGIN;

struct Fonts<'a> {
    definitions: &'a FontDefinitions,
    pdf: HashMap<String, Font>,
}

impl Fonts<'_> {
    fn glyph(&mut self, format: &TextFormat, ch: char) -> Result<(Font, u16), String> {
        let names = self.definitions.families.get(&format.font_id.family).ok_or("Missing font family")?;
        for name in names {
            let data = &self.definitions.font_data[name];
            let Ok(face) = ttf_parser::Face::parse(&data.font, data.index) else { continue };
            let Some(gid) = face.glyph_index(if ch == '\t' { ' ' } else { ch }) else { continue };
            let coords: Vec<_> =
                format.coords.as_ref().iter().map(|(tag, value)| (Tag::new(&tag.to_be_bytes()), *value)).collect();
            let key = format!("{name}:{coords:?}");
            if !self.pdf.contains_key(&key) {
                let font = Font::new_variable(Arc::new(data.font.to_vec()).into(), data.index, &coords)
                    .ok_or_else(|| format!("Invalid PDF font: {name}"))?;
                self.pdf.insert(key.clone(), font);
            }
            return Ok((self.pdf[&key].clone(), gid.0));
        }
        Err(format!("Missing PDF glyph: {ch}"))
    }
}

fn fill(surface: &mut Surface<'_>, color: Color32, text: bool) {
    // A white paper palette independent of the editor's dark canvas.
    let c = if text {
        Color32::from_rgb(25, 25, 25)
    } else if color == crate::md::CODE_BG || color == crate::md::BG {
        Color32::from_gray(245)
    } else if color == crate::md::BORDER {
        Color32::from_gray(180)
    } else {
        color
    };
    surface.set_fill(Some(Fill {
        paint: rgb::Color::new(c.r(), c.g(), c.b()).into(),
        opacity: krilla::num::NormalizedF32::new(color.a() as f32 / 255.0).unwrap(),
        ..Default::default()
    }));
}

fn text(
    surface: &mut Surface<'_>,
    shape: &egui::epaint::TextShape,
    fonts: &mut Fonts<'_>,
    top: f32,
    bottom: f32,
) -> Result<(), String> {
    let mut byte = 0;
    let mut section = 0;
    let sections = &shape.galley.job.sections;
    for row in &shape.galley.rows {
        // Keep runs together in the PDF, with the editor's exact advances. Unicode
        // ranges become ToUnicode mappings rather than outlined/rasterized letters.
        let mut run = String::new();
        let mut glyphs = Vec::new();
        let mut run_font: Option<Font> = None;
        let mut run_section = 0;
        let mut run_emoji = false;
        let mut origin = Point::from_xy(0.0, 0.0);
        let flush = |surface: &mut Surface<'_>,
                     run: &mut String,
                     glyphs: &mut Vec<KrillaGlyph>,
                     font: &Option<Font>,
                     si: usize,
                     origin| {
            if let Some(font) = font
                && !glyphs.is_empty()
            {
                let format = &sections[si].format;
                let color = if run.chars().all(|ch| crate::emoji::png(ch).is_some()) {
                    // The color overlay supplies visible ink; retain Unicode text for search/copy.
                    Color32::TRANSPARENT
                } else {
                    format.color
                };
                fill(surface, color, true);
                surface.draw_glyphs(origin, glyphs, font.clone(), run, format.font_id.size, false);
            }
            run.clear();
            glyphs.clear();
        };
        let mut previous_end = 0.0;
        for glyph in &row.glyphs {
            while section + 1 < sections.len() && byte >= sections[section].byte_range.end.0 {
                section += 1;
            }
            let format = &sections[section].format;
            byte += glyph.chr.len_utf8();
            if format.color.a() == 0
                || format.font_id.size < 1.0
                || shape.pos.y + row.pos.y < top - 0.1
                || shape.pos.y + row.pos.y >= bottom - 0.1
            {
                continue;
            }
            let rect = glyph.logical_rect().translate(shape.pos.to_vec2() + row.pos.to_vec2());
            if format.background.a() > 0 {
                let mut path = PathBuilder::new();
                path.push_rect(Rect::from_xywh(rect.min.x, rect.min.y, rect.width().max(0.01), rect.height()).unwrap());
                fill(surface, Color32::from_gray(240), false);
                surface.draw_path(&path.finish().unwrap());
            }
            for (stroke, y) in [(format.strikethrough, rect.center().y), (format.underline, rect.max.y)] {
                if stroke.width > 0.0 {
                    let mut path = PathBuilder::new();
                    path.push_rect(Rect::from_xywh(rect.min.x, y, rect.width().max(0.01), stroke.width).unwrap());
                    fill(surface, stroke.color, true);
                    surface.draw_path(&path.finish().unwrap());
                }
            }
            let (font, gid) = fonts.glyph(format, glyph.chr)?;
            let x = shape.pos.x + row.pos.x + glyph.pos.x;
            let y = shape.pos.y + row.pos.y + glyph.pos.y;
            let is_emoji = crate::emoji::png(glyph.chr).is_some();
            if run_font.as_ref() != Some(&font)
                || section != run_section
                || is_emoji != run_emoji
                || (x - previous_end).abs() > 0.1
            {
                flush(surface, &mut run, &mut glyphs, &run_font, run_section, origin);
                run_font = Some(font);
                run_section = section;
                run_emoji = is_emoji;
                origin = Point::from_xy(x, y);
            }
            let start = run.len();
            run.push(glyph.chr);
            glyphs.push(KrillaGlyph {
                glyph_id: GlyphId::new(gid as u32),
                text_range: start..run.len(),
                x_advance: glyph.advance_width / format.font_id.size,
                x_offset: 0.0,
                y_offset: 0.0,
                y_advance: 0.0,
                location: None,
            });
            previous_end = x + glyph.advance_width;
        }
        flush(surface, &mut run, &mut glyphs, &run_font, run_section, origin);
        if row.ends_with_newline {
            byte += 1;
        }
    }
    Ok(())
}

/// Page breaks fall between editor rows (including table rows and image reserves).
pub fn breaks(galley: &egui::Galley) -> Vec<f32> {
    let mut out = vec![0.0];
    for row in &galley.rows {
        let top = row.pos.y;
        let bottom = top + row.size.y;
        let mut start = *out.last().unwrap();
        if bottom > start + CONTENT_HEIGHT && top > start {
            out.push(top);
            start = top;
        }
        while bottom > start + CONTENT_HEIGHT {
            start += CONTENT_HEIGHT;
            out.push(start);
        }
    }
    out
}

pub fn pdf(ctx: &Context, mut output: egui::FullOutput, breaks: &[f32]) -> Result<Vec<u8>, String> {
    // PDF viewers antialias vector edges themselves. Atlas discs and feathered
    // triangles would turn circles into squares and leave seams on paper.
    ctx.tessellation_options_mut(|options| {
        options.feathering = false;
        options.prerasterized_discs = false;
    });
    let definitions = ctx.fonts(|f| f.definitions().clone());
    let mut fonts = Fonts { definitions: &definitions, pdf: HashMap::new() };
    let mut images = HashMap::new();
    for (id, deltas) in &output.textures_delta.set {
        let Some(delta) = deltas.first() else { continue };
        if *id == TextureId::default() {
            continue;
        }
        let egui::ImageData::Color(image) = &delta.image;
        let rgba = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
        images.insert(*id, krilla::image::Image::from_rgba8(rgba, image.size[0] as u32, image.size[1] as u32));
    }
    output.textures_delta.clear();
    let mut document = Document::new();
    for (page_index, top) in breaks.iter().enumerate() {
        let bottom = breaks.get(page_index + 1).copied().unwrap_or(top + CONTENT_HEIGHT);
        let mut page = document.start_page_with(PageSettings::from_wh(WIDTH, HEIGHT).unwrap());
        let mut surface = page.surface();
        let mut clip = PathBuilder::new();
        clip.push_rect(Rect::from_xywh(MARGIN - 12.0, MARGIN, COLUMN + 24.0, bottom - top).unwrap());
        surface.push_clip_path(&clip.finish().unwrap(), &Default::default());
        surface.push_transform(&Transform::from_translate(MARGIN, MARGIN - top));
        for clipped in &output.shapes {
            if clipped.shape.visual_bounding_rect().max.y < *top || clipped.shape.visual_bounding_rect().min.y >= bottom
            {
                continue;
            }
            if let Shape::Text(shape) = &clipped.shape {
                text(&mut surface, shape, &mut fonts, *top, bottom)?;
            } else {
                let meshes = ctx.tessellate(vec![clipped.clone()], 1.0);
                for primitive in meshes {
                    let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                        continue;
                    };
                    if let Some(image) = images.get(&mesh.texture_id) {
                        let bounds = mesh.calc_bounds();
                        surface.push_transform(&Transform::from_translate(bounds.min.x, bounds.min.y));
                        surface.draw_image(image.clone(), Size::from_wh(bounds.width(), bounds.height()).unwrap());
                        surface.pop();
                    } else {
                        let mut paths: Vec<(Color32, PathBuilder)> = Vec::new();
                        for triangle in mesh.indices.as_chunks::<3>().0 {
                            let vertices = triangle.map(|i| &mesh.vertices[i as usize]);
                            let color = vertices[0].color;
                            if color.a() == 0 {
                                continue;
                            }
                            if paths.last().is_none_or(|(c, _)| *c != color) {
                                paths.push((color, PathBuilder::new()));
                            }
                            let path = &mut paths.last_mut().unwrap().1;
                            path.move_to(vertices[0].pos.x, vertices[0].pos.y);
                            for v in &vertices[1..] {
                                path.line_to(v.pos.x, v.pos.y);
                            }
                            path.close();
                        }
                        for (color, path) in paths {
                            fill(&mut surface, color, false);
                            surface.draw_path(&path.finish().unwrap());
                        }
                    }
                }
            }
        }
        surface.pop();
        surface.pop();
        surface.finish();
        page.finish();
    }
    document.finish().map_err(|e| format!("{e:?}"))
}

/// The portal owns the print dialog and spooler; no printer CLI or shell pipeline.
#[cfg(target_os = "linux")]
pub fn print(bytes: &[u8], title: &str) -> Result<(), String> {
    use std::io::{Seek, Write};
    // Unlinked immediately: the portal receives an fd, not a persistent temp PDF.
    let (path, mut file) = temporary_pdf()?;
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_| file.rewind()).map_err(|e| e.to_string())?;
    pollster::block_on(async {
        let proxy = ashpd::desktop::print::PrintProxy::new().await?;
        // KDE requires a configured QPrinter. ashpd 0.11 serializes None as
        // token=0, so a direct Print request cannot open its print dialog.
        let setup = ashpd::desktop::print::PageSetup::default().width(210.0).height(297.0);
        let prepared = proxy.prepare_print(None, title, Default::default(), setup, None, true).await?.response()?;
        proxy.print(None, title, &file, Some(prepared.token), true).await?.response()
    })
    .or_else(|e| match e {
        ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled) => Ok(()),
        other => Err(other),
    })
    .map_err(|e| e.to_string())
}

fn temporary_pdf() -> Result<(std::path::PathBuf, std::fs::File), String> {
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let path = std::env::temp_dir().join(format!("mdedit-{}-{nonce}.pdf", std::process::id()));
    let file =
        std::fs::OpenOptions::new().create_new(true).read(true).write(true).open(&path).map_err(|e| e.to_string())?;
    Ok((path, file))
}

#[cfg(not(target_os = "linux"))]
pub fn print(bytes: &[u8], _title: &str) -> Result<(), String> {
    use std::io::Write;
    let (path, mut file) = temporary_pdf()?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    drop(file);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let escaped = path.to_string_lossy().replace('\'', "''");
        let status = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("Start-Process -FilePath '{escaped}' -Verb Print -ErrorAction Stop"),
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("PDF print handler failed".into());
        }
        // The associated viewer reads asynchronously; keep its input until OS temp cleanup.
        Ok(())
    }
    #[cfg(not(windows))]
    open::that(path).map_err(|e| e.to_string())
}
