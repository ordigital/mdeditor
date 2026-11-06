# MDEdit 0.1.9

- Opening an already open file with unsaved edits now asks whether to save them, while keeping the existing tab.
- Every five seconds, open files are checked for external content changes. Choose Reload, Save the editor version, or Do nothing. Dismissed changes stay quiet until the file changes again.
- Reload preserves tab syntax settings, handles Windows line endings and can be undone.
- Ctrl+click now follows links in rendered tables without moving the source cursor or entering table editing; ordinary clicks still edit tables.
- Added ♡ Buy me a coffee links to the status bar, About dialog and README.
- Updated dialogs in all 23 interface languages.

Validation: 80 tests passed; 3 manual benchmarks ignored; formatting, Clippy and release builds checked. Linux AppImage built on Ubuntu 22.04 (glibc 2.35); portable Windows ZIP cross-compiled with MinGW. Checksums and package contents verified. No interactive desktop or Windows/Wine runtime tests.
