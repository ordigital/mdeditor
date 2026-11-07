# Release workflow

Repository: `git@github.com:ordigital/mdeditor.git`, branch `master`. Releases use
annotated tags `v<version>` and titles `MDEdit <version>`. Version 0.1.10 follows
0.1.5's Linux AppImage + portable Windows ZIP format. Artifacts in `releases/`
are ignored by Git and uploaded to GitHub Releases instead.

1. Read `AGENTS.md` and `PROJECT_INDEX.yaml`. Check `git status`, the latest commit,
   remote branch and tags (`git ls-remote --heads --tags origin`). Preserve existing
   authorized work; do not force-push or overwrite old release assets.
2. Increment the package version in `Cargo.toml` and the `mdedit` package entry in
   `Cargo.lock`. Update documentation, release notes and the project index.
3. Run `cargo fmt --check`, `git diff --check`, `cargo test --offline --locked` and
   `cargo clippy --offline --locked --all-targets -- -D warnings`.
4. Build the local desktop executable with `cargo build --release --offline --locked`.
   The installed desktop launcher points at `/10tb/apps/MDEditor/target/release/mdedit`,
   an alias of this checkout. Test builds do not update that executable. Running
   editor windows must be restarted to use it.
5. Build Linux with `KEEP_BUILD=1 sh packaging/build-appimage.sh`. Requirements:
   running local Docker daemon, access to its socket, network, and `appimagetool`.
   Start Docker if needed (may require host/admin approval). Do not substitute a
   host-compiled binary: the Ubuntu 22.04 container keeps the glibc requirement at
   2.35. Docker volumes `mdedit-cargo` and `mdedit-rustup` cache dependencies/toolchain;
   `releases/build/target` caches compilation when KEEP_BUILD=1 is set.
6. Build Windows with `sh packaging/build-windows.sh`. Requirements: Python >=3.11,
   Rust target `x86_64-pc-windows-gnu` (`rustup target add x86_64-pc-windows-gnu`),
   MinGW-w64 (`x86_64-w64-mingw32-gcc`). The ZIP contains `MDEdit.exe`, `LICENSE` and bundled font/data licenses in
   `licenses/`; the script also writes its SHA-256 file. Builds use Cargo.lock.
7. In `releases/`, generate and check the AppImage checksum, substituting the actual
   version for 0.1.10:

   ```sh
   sha256sum MDEdit-0.1.10-x86_64.AppImage > MDEdit-0.1.10-x86_64.AppImage.sha256
   sha256sum -c MDEdit-0.1.10-x86_64.AppImage.sha256 MDEdit-0.1.10-windows-x86_64.zip.sha256
   ```

   Check ZIP CRCs/content with Python zipfile; compare the archived EXE with the
   compiled file. Check Linux GLIBC symbol requirements with `readelf --version-info
   releases/build/AppDir/usr/bin/mdedit` and Windows imports with
   `x86_64-w64-mingw32-objdump -p target/x86_64-pc-windows-gnu/release/mdedit.exe`.
   State runtime-test limits accurately: v0.1.5 was smoke-tested under Wine;
   v0.1.10 received static/package checks, without Windows/Wine runtime testing.
8. Finish index/notes, stage explicit files, commit, create the annotated version
   tag, and push branch and tag (no force):

   ```sh
   git tag -a v0.1.10 -m 'MDEdit 0.1.10'
   git push --atomic origin master v0.1.10
   gh release create v0.1.10 --repo ordigital/mdeditor --verify-tag --title 'MDEdit 0.1.10' --notes-file packaging/release-notes-0.1.10.md releases/MDEdit-0.1.10-x86_64.AppImage releases/MDEdit-0.1.10-x86_64.AppImage.sha256 releases/MDEdit-0.1.10-windows-x86_64.zip releases/MDEdit-0.1.10-windows-x86_64.zip.sha256
   ```

9. Verify the remote branch/tag and `gh release view v0.1.10 --repo
   ordigital/mdeditor --json url,tagName,isDraft,assets`; confirm all four assets
   are uploaded, their sizes/digests match local files, and `git status` is clean.
   A failed upload is recoverable; inspect release state before retrying, and do
   not recreate/replace a published release without checking what exists.

GitHub/SSH network calls and Docker may require execution outside the sandbox.
User requests to push and publish authorize those actions, but sandbox escalation
still applies. Never include tokens or credentials in the index, logs or notes.
