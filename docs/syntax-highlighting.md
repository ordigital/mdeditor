# Syntax highlighting architecture and measurements

Decision: syntect 5.3 + Oniguruma for HTML, CSS, PHP, Python, Rust, C, C++, Go and
JavaScript. Existing small configuration/Bash lexers remain in place. Both paths
produce the same sorted UTF-8 byte spans with palette colors. Fonts, wrapping,
cursor state and egui sections are downstream concerns.

## Why this engine

The standalone probe in `tools/syntax-probe` compared syntect with fancy-regex,
syntect with Oniguruma, and tree-sitter-highlight 0.27.1. Its fixture contains
HTML, CSS, JavaScript, PHP and Unicode. The final Tree-sitter comparison adds an
HTML injection for PHP `text` nodes, including `injection.combined`; upstream PHP
queries alone do substantially less work and are not a fair nested comparison.
Event/region counts are different representations, not a correctness score.

Release-mode results on Intel Core i9-13900KF, Rust 1.99.0, Linux; milliseconds,
single observations, not universal performance guarantees:

| Nested PHP fixture | syntect / fancy-regex | syntect / Oniguruma | Tree-sitter + HTML injections |
| --- | ---: | ---: | ---: |
| 100 KB, full analysis | 201.3 | 73.3 | 23.2 |
| 1 MB, full analysis | 2013.9 | 729.4 | 322.1 |
| 5 MB, full analysis | 10016.3 | 3668.5 | 3474.9 |

Tree-sitter is faster in full passes, especially small/medium files. Syntect was
selected because its grammar context stack already handles the required mixed
PHP/HTML/CSS/JS, and checkpoint-based incremental highlighting can be implemented
without maintaining custom injection queries and trees for multiple languages.
The simple highlighter API of Tree-sitter does not preserve the old parse tree
between calls. A fully incremental Tree-sitter integration remains a possible
future replacement if profiling justifies its added complexity.

Production syntect cache, the application's own small theme:

| PHP fixture | Cold analysis | Local edit | Lines reparsed |
| --- | ---: | ---: | ---: |
| 10 KB | 15.3 ms | 1.3 ms | 32 |
| 100 KB | 74.4 ms | 1.3 ms | 32 |
| 1 MB | 689.2 ms | 3.0 ms | 32 |
| 5 MB | 3513.4 ms | 17.2 ms | 32 |

These local edits converge quickly. An unterminated multiline string/comment can
require reparsing the entire suffix; the worker keeps that work off the UI thread.

## Caches and worker

- `highlight.rs:LANGUAGES` is the single registry of file extensions, special
  filenames, fenced aliases and syntect grammar keys. To add a supported grammar,
  add its `Syntax` variant and registry entry. Engine coverage tests ensure that
  every registered grammar exists in the bundled syntax set.
- `highlight::Analysis` owns the current text revision and colored byte spans per
  document. It sees the actual `TextBuffer` inside the layouter, so edits applied
  during `TextEdit.show()` cannot reuse an old revision by accident.
- `highlight/worker.rs` starts one lazy worker per application. Its queue keeps
  at most the latest request for each document, cancels obsolete work between
  lines, and notifies egui after finishing. Results are accepted only for their
  exact document/revision. Closed documents' results are ignored; their worker
  caches are pruned when later jobs are processed.
- During analysis, only unchanged prefix/suffix colors are retained, with byte
  offsets translated through the edit at UTF-8 boundaries. Inserted/replaced text
  starts plain. These are provisional colors: changed lexical context is corrected
  by the current revision's result. Old, unmapped offsets are never reused.
- `highlight/engine.rs` owns syntect on the worker thread, including all parser
  state. It checkpoints parser/highlighter state every 32 lines, rewinds before
  an edit, and reuses an unchanged suffix only after both state and text converge.
- Analysis survives width/font/cursor changes. Code cursor position is absent
  from the galley cache key; Markdown still reveals syntax on the cursor line.
- `highlight::LayoutCache` retains unchanged line galleys independently of egui's
  per-frame font cache. Changed wrapping, fonts, language fallback, DPI or font
  weights invalidate the appropriate layout cache.
- Markdown live preview remains intact. The worker uses the shared `Fence` and
  front-matter rules to color supported fenced block bodies. `md::build_colored`
  applies those spans only to code lines, preserving Markdown decorations and
  table/math handling. Unknown fenced languages remain plain code.

## Whole-editor CPU measurements

Headless egui test frames in release mode, excluding disk I/O, GPU submission,
compositor and monitor latency. Cursor percentiles use 40 frames per file.
Fixtures are synthetic/repetitive; compare real files before making broader claims.

Before this integration, the Bash fixture at about 1 MB took 28.1–29.5 ms for
cursor movement between lines and 59.4 ms for one edit. Afterward:

| File | Cursor p95 / p99 | First edit frame | Edit to updated colors |
| --- | ---: | ---: | ---: |
| Bash 1 MB | 0.252 / 1.859 ms | 29.9 ms | synchronous |
| Nested PHP 10 KB | 0.046 / 0.053 ms | 0.4 ms | 1.6 ms |
| Nested PHP 100 KB | 0.057 / 0.087 ms | 2.7 ms | 4.8 ms |
| Nested PHP 1 MB | 0.294 / 0.396 ms | 28.2 ms | 53.9 ms |

PHP first plain frames took 2.2/2.4/5.7 ms for 10 KB/100 KB/1 MB; complete initial
colors arrived after 23.5/89.6/759.8 ms. Full opening latency and frame costs still
vary with font initialization and line wrapping.

The complete sequential microbenchmark process peaked at 176680 KiB RSS (about
172.5 MiB), including font caches, source copies and the 5 MB parser stress case.
This is not a per-document memory estimate or a baseline memory comparison.

The original integration benchmark executables were 27782592 bytes on Linux and
20476928 bytes on Windows. Oniguruma is linked into them: ELF dependencies and PE
imports show no extra libonig DLL/shared library requirement. These are executable
sizes, not AppImage/ZIP sizes. The integration is included in release 0.1.7.

The single multiline `TextEdit` still builds/scans/concatenates full-document
layout information. Editing a 1 MB file does not reliably fit a 16.7 ms frame
budget. An incremental parser alone cannot remove that limitation. A rope or
virtualized editor would be a separate project; neither is introduced here.

## Limits and fallback

Syntect analysis falls back to plain text for a file/block containing a line over
262144 bytes. This protects the worker from expensive regexes on minified or
pathological lines; editing and wrapping still work. Cancellation is checked
between lines, not inside a regex call. First colors are asynchronous. No external
process executes source code; no LSP, plugin loader or additional editing mode is
introduced. Existing config/Bash lexical behavior is preserved.

## Reproduction and validation

```sh
cargo test --offline --locked
cargo clippy --offline --locked --all-targets -- -D warnings
cargo test --release --offline --locked benchmark_ -- --ignored --nocapture --test-threads=1
cargo run --release --offline --locked --manifest-path tools/syntax-probe/Cargo.toml
cargo run --release --offline --locked --no-default-features --features onig --manifest-path tools/syntax-probe/Cargo.toml
cargo build --release --offline --locked
cargo build --release --offline --locked --target x86_64-pc-windows-gnu
```

The probe's Tree-sitter dependencies are development tools, not production
application dependencies. Raw measurements are in `docs/benchmarks/`. Baseline
measurements were captured locally before the cache/worker integration. The early
fancy-regex probe used upstream PHP queries only for its Tree-sitter column; use
the final Oniguruma probe for the comparable injected Tree-sitter results.

Regression tests cover mixed PHP/HTML/CSS/JS scopes, UTF-8 ranges, incremental
results versus a fresh parse, multiline propagation, unknown/four-character
fences, async Markdown layout, stale results, provisional remapping, font/wrap
invalidation and cursor cache reuse. Linux and MinGW Windows builds are checked;
Windows runtime behavior still requires testing on Windows.

References:

- https://docs.rs/syntect/5.3.0/syntect/parsing/struct.ParseState.html
- https://docs.rs/syntect/5.3.0/syntect/easy/struct.HighlightLines.html
- https://docs.rs/tree-sitter-highlight/0.27.1/src/tree_sitter_highlight/highlight.rs.html
- https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html#language-injection
