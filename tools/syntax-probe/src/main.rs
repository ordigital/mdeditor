use std::time::Instant;
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings};
use tree_sitter_highlight::{HighlightConfiguration, Highlighter};

const SAMPLE: &str = "<style>body { color: red; }</style>\n<script>const message = 'żółw🙂'; console.log(message);</script>\n<div><?php echo $name; ?></div>\n";
fn config(
    language: tree_sitter_language::LanguageFn,
    name: &str,
    highlights: &str,
    injections: &str,
) -> HighlightConfiguration {
    let mut c = HighlightConfiguration::new(language.into(), name, highlights, injections, "").unwrap();
    c.configure(&["keyword", "string", "comment", "tag", "property", "function", "variable", "number", "operator"]);
    c
}
fn main() {
    let start = Instant::now();
    let syntaxes = SyntaxSet::load_defaults_newlines();
    let themes = ThemeSet::load_defaults();
    let theme = &themes.themes["base16-ocean.dark"];
    println!("syntect init_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    for ext in ["html", "css", "php", "py", "rs", "c", "cpp", "go", "js", "sh"] {
        println!("syntax {ext}: {:?}", syntaxes.find_syntax_by_extension(ext).map(|s| &s.name));
    }
    let start = Instant::now();
    let html = config(
        tree_sitter_html::LANGUAGE,
        "html",
        tree_sitter_html::HIGHLIGHTS_QUERY,
        tree_sitter_html::INJECTIONS_QUERY,
    );
    let css = config(tree_sitter_css::LANGUAGE, "css", tree_sitter_css::HIGHLIGHTS_QUERY, "");
    let js = config(tree_sitter_javascript::LANGUAGE, "javascript", tree_sitter_javascript::HIGHLIGHT_QUERY, "");
    let php_injections = format!(
        "{}\n((text) @injection.content (#set! injection.language \"html\") (#set! injection.combined))",
        tree_sitter_php::INJECTIONS_QUERY
    );
    let php = config(tree_sitter_php::LANGUAGE_PHP, "php", tree_sitter_php::HIGHLIGHTS_QUERY, &php_injections);
    println!("tree-sitter config_ms={:.3}", start.elapsed().as_secs_f64() * 1000.0);
    for target in [10_000, 100_000, 1_000_000, 5_000_000] {
        let text = SAMPLE.repeat(target / SAMPLE.len() + 1);
        for (name, syntax) in [("html", &html), ("php", &php)] {
            let start = Instant::now();
            let mut highlighter = HighlightLines::new(syntaxes.find_syntax_by_extension(name).unwrap(), theme);
            let regions: usize = LinesWithEndings::from(&text)
                .map(|line| highlighter.highlight_line(line, &syntaxes).unwrap().len())
                .sum();
            println!(
                "syntect {name} bytes={} ms={:.3} regions={regions}",
                text.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
            let start = Instant::now();
            let mut highlighter = Highlighter::new();
            let events = highlighter
                .highlight(syntax, text.as_bytes(), None, None, |lang| match lang {
                    "css" => Some(&css),
                    "javascript" => Some(&js),
                    "html" => Some(&html),
                    "php" => Some(&php),
                    _ => None,
                })
                .unwrap()
                .map(|event| event.unwrap())
                .count();
            println!(
                "tree-sitter {name} bytes={} ms={:.3} events={events}",
                text.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }
}
