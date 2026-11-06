## What's new

- View width replaces Optimize: Narrow, Medium, Wide and Full, with F2 to cycle. Intermediate widths evenly expand the original reading column; Full matches the previous unrestricted view. Existing settings migrate automatically. All 23 UI languages are supported.
- Slightly larger text in syntax-colored code files.
- HTML, CSS, PHP, Python, Rust, C, C++, Go and JavaScript highlighting, including embedded languages and fenced Markdown blocks. Background analysis and line caches keep cursor movement fast. Bash highlighting also added.
- Ctrl+Shift+K creates and links a new Markdown page from selected text. A centered formatting toolbar exposes all existing formatting actions with font-independent vector icons.
- Better nested quote indentation and compact Shift+Enter continuation spacing.
- Open defaults to all files with an optional Markdown filter. Saving preserves an explicitly supplied extension and only adds .md when no extension is given.
- File drag/drop opens tabs. Linux prefers X11/XWayland when DISPLAY is available, working around missing file-drop events in winit 0.30's native Wayland backend. Wayland-only systems retain that upstream limitation.
- Shared Markdown parsing rules reduce duplication between editing and rendering. Updated English/Polish documentation and agent index.

## Downloads

- **Linux x86_64**: `MDEdit-0.1.7-x86_64.AppImage`, built on Ubuntu 22.04 (glibc 2.35+). Make executable and run; without FUSE 2 use `--appimage-extract-and-run`.
- **Windows x86_64**: `MDEdit-0.1.7-windows-x86_64.zip`; unzip and run `MDEdit.exe`.

SHA-256 checksums are attached. 76 automated tests pass; three manual benchmarks are ignored by the standard test run. Formatting and Clippy checks pass. Packages receive checksum/content and ELF/PE dependency checks. Windows is cross-compiled and has not been runtime-tested on Windows/Wine; interactive desktop drag/drop was not tested automatically. No code signing. Linux single-instance behavior requires closing all running windows before a new binary takes effect.
