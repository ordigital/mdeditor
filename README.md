# MDEdit

[♡ Buy me a coffee](https://buymeacoffee.com/adamstrad)

A fast, minimal **WYSIWYG Markdown editor** written in Rust with [egui](https://github.com/emilk/egui). It works like Obsidian's live preview: markers are hidden everywhere except on the line with the cursor, so `# Title` becomes a heading as soon as you move away from it. Dark mode only. The UI is available in 23 languages (choose one in Preferences).

Polish version: [README.pl.md](README.pl.md).

![MDEdit](docs/screenshot.png)

## Features

- **File → Export PDF… / Print…** share an A4 document with selectable, searchable Unicode text, Markdown formatting, tables and local/embedded images. Export includes unsaved edits. Linux printing uses the XDG print portal; Windows uses the registered PDF application’s print action.

- **Live preview** of headings, bold, italic, strikethrough, `==highlight==`, inline code and code
  blocks, links, `[[wiki links]]`, `#tags`, footnotes, `%%comments%%`, horizontal rules, front matter.
- **Lists and tasks** with hanging indents, clickable checkboxes, automatic numbering, Enter continues a list, Shift+Enter continues text within an item, Tab / Shift+Tab indent nested lists.
- **Quotes and callouts** (`> [!note]`, `tip`, `warning`, `danger`, `question`, …), **tables** with column alignment.
- **Formulas**: `$…$`, `\(…\)`, `$$…$$`, `\[…\]`, including `\boxed{}` and `\text{}`, rendered as Unicode.
- **Images**: from files, embedded as `data:` base64, `![[embeds]]`, `![alt|300](…)` widths.
  Ctrl+V with an image in the clipboard embeds it as a JPEG (85%, max 1200 px wide), or saves a WebP next to the document.
- **Tabs, find / replace, undo / redo** (10 steps), recent files, a simple **file panel** (create, open, delete to trash).
- A formatting toolbar below the tabs, including headings 1–6, normal text, inline styles, links, lists, tasks, quotes and images.
- Ctrl+Shift+K creates a new Markdown file in the current document folder and links the selected text to it. Enter or Save confirms; existing files are never overwritten.
- Formatting shortcuts that also toggle the selection (Ctrl+B, Ctrl+I, …), Ctrl+scroll zooms the font.
- **View width**: Narrow, Medium, Wide and Full; F2 cycles between them. Narrow preserves the original reading column; Full uses the available editor width. Code files use a slightly larger font.
- Drop files onto the window to open them in tabs. The Open dialog shows all files by default, with an optional Markdown filter. Linux prefers X11/XWayland when available because winit 0.30 cannot receive native Wayland file drops.
- **Single instance**: running `mdedit file.md` again opens the file in a new tab of the running window.
- Linux primary selection: select text, paste with the middle mouse button or Shift+Insert.
- Defaults from Obsidian's dark theme: Inter (text) and Source Code Pro (code), both embedded;
  UI in the desktop font (KDE / GNOME settings, Noto Sans 10 as fallback).
- Config files are shown as syntax-colored plain text, chosen by extension: XML, TOML, JSON, YAML, INI/CFG/CONF and `.env`. Bash scripts (`.sh`, `.bash`) and shell startup files such as `.bashrc` and `.profile` also have syntax coloring.
- **Syntax menu**: choose Automatic, Markdown or any supported code/config language for the current tab. A manual choice survives saving and Save As until you select Automatic or close the tab; it does not change the filename or contents.
- **Code syntax highlighting** for HTML, CSS, PHP, Python, Rust, C, C++, Go and JavaScript, including HTML/CSS/JS embedded in PHP and supported fenced Markdown blocks. Grammar analysis runs in the background and reuses unchanged lines.
  Very long code lines (over 256 KiB) fall back to plain text. See [architecture and measurements](docs/syntax-highlighting.md).
- Binary and very large files (over 5 MB) are refused instead of freezing the editor.
- Windows line endings (CRLF) are kept when saving. Ctrl+click opens only web and mail links, and never programs or scripts.

`examples/example.md` (English) and `examples/przyklad.md` (Polish) show every supported element.

## Build and run

Requires Rust 1.95 or newer (edition 2024).

```sh
cargo run --release -- examples/example.md
# or
cargo build --release && ./target/release/mdedit notes.md
cargo test --release
```

On Linux you need the usual windowing libraries (Wayland and/or X11, `libxkbcommon`) and, for the file dialogs, an XDG desktop portal (KDE, GNOME, …). Clipboard images on Wayland use `wl-clipboard` protocols built into the binary.

### Launcher entry and icon

```sh
./packaging/install.sh
```

installs `mdedit.desktop` and the icon for the current user. On Wayland the taskbar icon comes from that file (it is matched by the app id `mdedit`); on X11 and Windows the icon is also embedded in the window.

## Keyboard shortcuts

| Action | Keys |
| --- | --- |
| Print | Ctrl+P |
| Export PDF | Ctrl+Shift+E |
| New / Open / Save / Save as | Ctrl+N (Ctrl+T) / Ctrl+O / Ctrl+S / Ctrl+Shift+S |
| Save all | Ctrl+Shift+Alt+S (Ctrl+Shift+Cmd+S on macOS)¹ |
| Close / Close all / Exit | Ctrl+W / Ctrl+Shift+W / Ctrl+Q |
| Undo / Redo | Ctrl+Z / Ctrl+Y, Ctrl+Shift+Z |
| Copy / Paste (text or image) | Ctrl+C / Ctrl+V |
| Paste selection (primary) | Middle click, Shift+Insert |
| Find / Replace | Ctrl+F |
| Insert image from file | Ctrl+Shift+I |
| File panel | Ctrl+Shift+F |
| Preferences / About | Ctrl+, / F1 |
| Bold / Italic / Strikethrough | Ctrl+B / Ctrl+I / Ctrl+D |
| Inline code / Highlight / Link | Ctrl+E / Ctrl+Shift+H / Ctrl+K |
| Create a new linked page | Ctrl+Shift+K |
| Heading 1–6 / normal text | Ctrl+1 … Ctrl+6 / Ctrl+0 |
| Bullet list / Task / Quote | Ctrl+Shift+L / Ctrl+L / Ctrl+Shift+Q |
| Open link or jump to footnote | Ctrl+click |
| Change font size | Ctrl+scroll |
| Cycle view width | F2 |
| New list item / line within item | Enter / Shift+Enter |
| Indent list item | Tab / Shift+Tab |

¹ egui does not report the Super/Meta key on Linux and Windows, so Alt stands in for it.

## Preferences

Edit → Preferences… (Ctrl+,): editor font, text / bold / code weight, text gamma, font size, line height,
how pasted images are stored and the language. **Reset** restores the default font settings (Inter, weights
300 / 600 / 400, gamma 0.80, 17 px, line height 1.65). View → View width offers Narrow, Medium, Wide and Full, plus toggles for
line numbers, inline title and the file panel. Existing readable-width settings migrate automatically.

Settings are stored in `~/.config/mdedit/config`, recent files in `~/.config/mdedit/recent`.

## Project layout

| File | Purpose |
| --- | --- |
| `src/main.rs` | entry point, single-instance socket, window icon |
| `src/app.rs` | the application: menus, tabs, editor, dialogs, undo, settings |
| `src/md.rs` | Markdown to styled text layout and decorations |
| `src/markdown.rs` | shared Markdown prefix, indentation and block-boundary rules |
| `src/latex.rs` | LaTeX math to Unicode |
| `src/highlight.rs`, `src/highlight/` | language registry, shared spans/layout, configuration and Bash lexers, background incremental grammar engine |
| `src/files.rs` | file panel |
| `src/fonts.rs` | font loading (embedded Inter / Source Code Pro, system fonts via fontconfig) |
| `src/base64.rs`, `src/i18n.rs` | base64 for embedded images, English / Polish strings |

## License and credits

MIT, see [LICENSE](LICENSE). Created by ORDIGITAL Adam Mateusz Brożyński.

Code written with the help of Claude/Codex.

Fonts: [Inter](https://github.com/rsms/inter) and [Source Code Pro](https://github.com/adobe-fonts/source-code-pro),
both under the SIL Open Font License 1.1 (see `assets/`). Colors and proportions follow the default dark theme of Obsidian; this project is not affiliated with Obsidian.
