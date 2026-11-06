## What's new

- New **Syntax** menu: Automatic, Markdown, XML, TOML, JSON, YAML, INI, .env, Bash, HTML, CSS, PHP, Python, Rust, C, C++, Go and JavaScript.
- Override highlighting for the current tab, including unsaved files. A manual selection survives Save/Save As; Automatic restores filename-based detection. Closing/reopening a file starts in Automatic mode.
- Switching syntax refreshes highlighting and the relevant Markdown editing controls without changing text, filename or undo history.
- Menu entries use the existing language registry, keeping future language additions in one place. Menu labels translated across all 23 UI languages.
- Updated English/Polish documentation and the agent index, including a mandatory index maintenance rule.

## Downloads and verification

- **Linux x86_64**: `MDEdit-0.1.8-x86_64.AppImage`, built on Ubuntu 22.04 (glibc 2.35+). Make executable and run; without FUSE 2 use `--appimage-extract-and-run`.
- **Windows x86_64**: `MDEdit-0.1.8-windows-x86_64.zip`; unzip and run `MDEdit.exe`.

SHA-256 checksums are attached. 78 automated tests pass; three manual benchmarks are ignored. Formatting, Clippy and package integrity/dependency checks pass. Changes reviewed against DRY, YAGNI and KISS: no new dependency, parser or persisted preference is needed for manual selection. Windows cross-compiled; Windows/Wine runtime and interactive desktop behavior were not tested. Existing native-Wayland file-drop limitation remains on systems without XWayland. Close all running editor windows before starting the updated binary.
