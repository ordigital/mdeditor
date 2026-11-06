## What's new

- Automatic ordered-list numbering during editing, including nested lists and find/replace. Tabs and spaces representing the same indentation share a counter.
- Shift+Enter adds an indented line inside the current list item; Enter creates the next item. Tab / Shift+Tab change the nesting level.
- Stable vertical spacing for empty bullet items.
- Ctrl+T opens a new tab, alongside Ctrl+N.
- About credits updated to Claude/Codex in all supported UI languages.
- Image loading respects the graphics device's texture limit. Numbering updates avoid repeatedly scanning the document to map cursor positions.
- Added a project index for agents, release instructions and a Windows packaging script.

## Downloads

- **Linux (x86_64)**: `MDEdit-0.1.6-x86_64.AppImage`. Built on Ubuntu 22.04; requires glibc 2.35 or newer. Make executable with `chmod +x` and run it. If FUSE 2 is missing, use `--appimage-extract-and-run`.
- **Windows (x86_64)**: `MDEdit-0.1.6-windows-x86_64.zip`. Unzip and run `MDEdit.exe`.

The Windows executable was cross-compiled on Linux. Package integrity and DLL imports were checked; this version has not been runtime-tested on Windows or Wine. It is not code-signed. On Windows a second launch opens a separate window, and the editor font is limited to built-in Inter.

SHA-256 checksums are attached. All 46 automated tests pass; formatting and Clippy checks pass.
