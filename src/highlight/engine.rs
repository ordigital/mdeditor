//! Grammar state belongs to the analysis worker, never the UI/rendering code.
use super::{Span, Syntax, push_span};
use crate::{markdown::Fence, md};
use std::{collections::BTreeMap, sync::Arc};
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, HighlightState, StyleModifier, Theme, ThemeItem},
    parsing::{ParseState, SyntaxSet},
};

const CHECKPOINT_LINES: usize = 32;
// Regex grammars on minified/unbroken lines must not hold up newer editing requests.
const MAX_LINE_BYTES: usize = 262_144;
type State = (HighlightState, ParseState);

pub(super) struct Engine {
    syntaxes: SyntaxSet,
    theme: Theme,
}

impl Engine {
    pub fn new() -> Self {
        let convert = |c: eframe::egui::Color32| Color { r: c.r(), g: c.g(), b: c.b(), a: 255 };
        let mut theme = Theme::default();
        theme.settings.foreground = Some(convert(md::TEXT));
        for (selectors, color) in [
            ("comment, comment punctuation", md::DIM),
            ("string, string punctuation", md::TAG),
            ("keyword, storage", md::CODE),
            ("constant", md::CODE),
            ("entity.name, support.function, variable", md::LINK),
            ("entity.name.tag", super::PINK),
            ("punctuation", md::MUTED),
        ] {
            theme.scopes.push(ThemeItem {
                scope: selectors.parse().expect("static scope selectors"),
                style: StyleModifier { foreground: Some(convert(color)), ..Default::default() },
            });
        }
        Self { syntaxes: SyntaxSet::load_defaults_newlines(), theme }
    }
}

#[derive(Default)]
struct CodeCache {
    syntax: Option<Syntax>,
    source: Arc<str>,
    lines: Vec<Arc<[Span]>>,
    checkpoints: BTreeMap<usize, State>,
    parsed_lines: usize,
}

impl CodeCache {
    /// Rewind before the edit, then stop once unchanged suffix + parser state converge.
    fn analyze(
        &mut self,
        engine: &Engine,
        syntax: Syntax,
        text: Arc<str>,
        cancelled: &impl Fn() -> bool,
    ) -> Option<Vec<Span>> {
        if syntax.language().engine.is_none() {
            return Some(super::native_spans(&text, syntax));
        }
        let new: Vec<&str> = text.split_inclusive('\n').collect();
        if new.iter().any(|line| line.len() > MAX_LINE_BYTES) {
            return Some(Vec::new());
        }
        if self.syntax != Some(syntax) {
            *self = Self { syntax: Some(syntax), ..Default::default() };
        }
        let grammar = engine.syntaxes.find_syntax_by_extension(syntax.language().engine?)?;
        let old: Vec<&str> = self.source.split_inclusive('\n').collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        let (rewind, mut state) = self
            .checkpoints
            .range(..=prefix)
            .next_back()
            .map(|(i, s)| (*i, s.clone()))
            .unwrap_or_else(|| (0, HighlightLines::new(grammar, &engine.theme).state()));
        let mut output = self.lines[..rewind].to_vec();
        let mut checkpoints: BTreeMap<usize, State> =
            self.checkpoints.range(..rewind).map(|(i, s)| (*i, s.clone())).collect();
        let mut parsed = 0;
        for (i, line) in new.iter().enumerate().skip(rewind) {
            if cancelled() {
                return None;
            }
            if i >= new.len() - suffix {
                let old_i = old.len() - (new.len() - i);
                if self.checkpoints.get(&old_i) == Some(&state) {
                    output.extend_from_slice(&self.lines[old_i..]);
                    checkpoints
                        .extend(self.checkpoints.range(old_i..).map(|(index, s)| (i + index - old_i, s.clone())));
                    break;
                }
            }
            if i % CHECKPOINT_LINES == 0 || i == rewind {
                checkpoints.insert(i, state.clone());
            }
            let mut h = HighlightLines::from_state(&engine.theme, state.0, state.1);
            let regions = h.highlight_line(line, &engine.syntaxes).ok()?;
            let mut spans = Vec::new();
            let mut offset = 0;
            for (style, part) in regions {
                let color = eframe::egui::Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                push_span(&mut spans, offset..offset + part.len(), color);
                offset += part.len();
            }
            state = h.state();
            output.push(spans.into());
            parsed += 1;
        }
        if cancelled() {
            return None;
        }
        let mut spans = Vec::new();
        let mut offset = 0;
        for (line, tokens) in new.iter().zip(&output) {
            for span in tokens.iter() {
                push_span(&mut spans, offset + span.range.start..offset + span.range.end, span.color);
            }
            offset += line.len();
        }
        self.source = text;
        self.lines = output;
        self.checkpoints = checkpoints;
        self.parsed_lines = parsed;
        Some(spans)
    }
}

#[derive(Default)]
pub(super) struct Document {
    blocks: Vec<CodeCache>,
}

impl Document {
    pub fn analyze(
        &mut self,
        engine: &Engine,
        syntax: Option<Syntax>,
        text: Arc<str>,
        cancelled: impl Fn() -> bool,
    ) -> Option<Vec<Span>> {
        if let Some(syntax) = syntax {
            self.blocks.resize_with(1, CodeCache::default);
            return self.blocks[0].analyze(engine, syntax, text, &cancelled);
        }
        let mut spans = Vec::new();
        let mut open: Option<(Fence, Option<Syntax>, usize)> = None;
        let mut offset = 0;
        let mut block = 0;
        let front = crate::markdown::front_matter_end(&text);
        for (li, line) in text.split_inclusive('\n').enumerate() {
            if cancelled() {
                return None;
            }
            if front.is_some_and(|end| li <= end) {
                offset += line.len();
                continue;
            }
            if let Some((fence, syntax, start)) = open {
                if fence.closes(line) {
                    if let Some(syntax) = syntax {
                        self.block(engine, syntax, &text, start..offset, block, &cancelled, &mut spans)?;
                        block += 1;
                    }
                    open = None;
                }
            } else if let Some(fence) = Fence::opening(line) {
                let alias = fence.info(line).split_whitespace().next().unwrap_or("");
                open = Some((fence, Syntax::from_alias(alias), offset + line.len()));
            }
            offset += line.len();
        }
        if let Some((_, Some(syntax), start)) = open {
            self.block(engine, syntax, &text, start..text.len(), block, &cancelled, &mut spans)?;
            block += 1;
        }
        self.blocks.truncate(block);
        Some(spans)
    }

    #[allow(clippy::too_many_arguments)]
    fn block(
        &mut self,
        engine: &Engine,
        syntax: Syntax,
        text: &str,
        range: std::ops::Range<usize>,
        index: usize,
        cancelled: &impl Fn() -> bool,
        spans: &mut Vec<Span>,
    ) -> Option<()> {
        self.blocks.resize_with((index + 1).max(self.blocks.len()), CodeCache::default);
        let tokens = self.blocks[index].analyze(engine, syntax, Arc::from(&text[range.clone()]), cancelled)?;
        for token in tokens {
            push_span(spans, range.start + token.range.start..range.start + token.range.end, token.color);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tokens(engine: &Engine, syntax: Syntax, text: &str) -> Vec<Span> {
        CodeCache::default().analyze(engine, syntax, Arc::from(text), &|| false).unwrap()
    }
    fn at(spans: &[Span], text: &str, needle: &str) -> eframe::egui::Color32 {
        let i = text.find(needle).unwrap();
        spans.iter().find(|s| s.range.contains(&i)).unwrap().color
    }

    #[test]
    #[ignore = "manual release-mode incremental engine benchmark"]
    fn benchmark_incremental_engine() {
        let engine = Engine::new();
        let sample = "<style>body { color: red; }</style>\n<script>const message = 'żółw🙂'; console.log(message);</script>\n<div><?php echo $name; ?></div>\n";
        for bytes in [10_000, 100_000, 1_000_000, 5_000_000] {
            let text = sample.repeat(bytes / sample.len() + 1);
            let mut cache = CodeCache::default();
            let start = std::time::Instant::now();
            cache.analyze(&engine, Syntax::Php, Arc::from(text.as_str()), &|| false).unwrap();
            let cold = start.elapsed().as_secs_f64() * 1000.0;
            let mut edited = text.clone();
            let at = text.len() / sample.len() / 2 * sample.len();
            edited.insert_str(at, "<!-- edit -->");
            let start = std::time::Instant::now();
            cache.analyze(&engine, Syntax::Php, Arc::from(edited.as_str()), &|| false).unwrap();
            eprintln!(
                "incremental php bytes={} cold_ms={cold:.3} edit_ms={:.3} parsed_lines={}",
                text.len(),
                start.elapsed().as_secs_f64() * 1000.0,
                cache.parsed_lines
            );
        }
    }

    #[test]
    fn grammar_registry_covers_requested_languages() {
        let engine = Engine::new();
        for language in super::super::LANGUAGES.iter().filter(|l| l.engine.is_some()) {
            assert!(
                engine.syntaxes.find_syntax_by_extension(language.engine.unwrap()).is_some(),
                "{:?}",
                language.syntax
            );
            assert_eq!(
                Syntax::from_path(std::path::Path::new(&format!("test.{}", language.extensions[0]))),
                Some(language.syntax)
            );
            assert_eq!(Syntax::from_alias(language.aliases[0]), Some(language.syntax));
        }
    }

    #[test]
    fn php_restores_html_css_and_javascript_contexts() {
        let engine = Engine::new();
        let text = "<style>/* css-before */ .card { color: <?php echo $color; ?>; /* css-after */ }</style>\n<script>const x = 'żółw🙂'; // js-comment\n</script>\n<?php echo 'php-string'; ?>";
        let spans = tokens(&engine, Syntax::Php, text);
        for comment in ["/* css-before", "/* css-after", "// js-comment"] {
            assert_eq!(at(&spans, text, comment), md::DIM, "{comment}");
        }
        assert_ne!(at(&spans, text, "echo"), md::TEXT);
        assert_eq!(at(&spans, text, "'php-string'"), md::TAG);
        assert_eq!(at(&spans, text, "'żółw"), md::TAG);
        assert!(spans.iter().all(|s| text.is_char_boundary(s.range.start) && text.is_char_boundary(s.range.end)));
    }

    #[test]
    fn incremental_edits_match_fresh_parse_and_stop_on_unchanged_suffix() {
        let engine = Engine::new();
        let mut cache = CodeCache::default();
        let original = "let value = 42; // stable\n".repeat(320);
        cache.analyze(&engine, Syntax::Rust, Arc::from(original.as_str()), &|| false).unwrap();
        let position = original.find("stable").unwrap() + "let value = 42; // stable\n".len() * 100;
        let mut text = original.clone();
        text.insert_str(position, "żółw🙂");
        let updated = cache.analyze(&engine, Syntax::Rust, Arc::from(text.as_str()), &|| false).unwrap();
        assert_eq!(updated, tokens(&engine, Syntax::Rust, &text));
        assert!(cache.parsed_lines < 70, "reparsed {} lines", cache.parsed_lines);
        text.insert_str(position, "\n/* multiline\n");
        let updated = cache.analyze(&engine, Syntax::Rust, Arc::from(text.as_str()), &|| false).unwrap();
        assert_eq!(updated, tokens(&engine, Syntax::Rust, &text));
        text.push_str("\n*/\nfn done() {}\n");
        let updated = cache.analyze(&engine, Syntax::Rust, Arc::from(text.as_str()), &|| false).unwrap();
        assert_eq!(updated, tokens(&engine, Syntax::Rust, &text));
    }

    #[test]
    fn markdown_fences_use_the_same_engine_and_absolute_byte_offsets() {
        let engine = Engine::new();
        let text = "żółw🙂\n````python\n# inner-comment\nprint('hello')\n```\n# still-inner\n````\nordinary prose\n~~~unknown\n# plain\n~~~\n";
        let spans = Document::default().analyze(&engine, None, Arc::from(text), || false).unwrap();
        assert_eq!(at(&spans, text, "# inner-comment"), md::DIM);
        assert_eq!(at(&spans, text, "# still-inner"), md::DIM);
        for outside in ["żółw", "ordinary prose", "# plain"] {
            assert!(!spans.iter().any(|s| s.range.contains(&text.find(outside).unwrap())));
        }
        assert!(spans.iter().all(|s| text.is_char_boundary(s.range.start) && text.is_char_boundary(s.range.end)));
    }

    #[test]
    fn very_long_lines_degrade_to_plain_text_and_cancellation_keeps_prior_state() {
        let engine = Engine::new();
        let mut cache = CodeCache::default();
        assert!(
            cache
                .analyze(&engine, Syntax::Html, Arc::from("x".repeat(MAX_LINE_BYTES + 1)), &|| false)
                .unwrap()
                .is_empty()
        );
        assert!(cache.analyze(&engine, Syntax::Rust, Arc::from("fn main() {}"), &|| true).is_none());
        assert_eq!(
            cache.analyze(&engine, Syntax::Rust, Arc::from("fn main() {}"), &|| false).unwrap(),
            tokens(&engine, Syntax::Rust, "fn main() {}")
        );
    }
}
