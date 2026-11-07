#!/bin/sh
# Cross-compile the portable Windows x86_64 ZIP on Linux.
# Requires rustup target x86_64-pc-windows-gnu, MinGW-w64 GCC and Python 3.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
cargo build --release --locked --target x86_64-pc-windows-gnu
python3 - <<'PY'
import hashlib
import pathlib
import tomllib
import zipfile

root = pathlib.Path.cwd()
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
archive = root / 'releases' / f'MDEdit-{version}-windows-x86_64.zip'
archive.parent.mkdir(exist_ok=True)
with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as out:
    out.write(root / 'target/x86_64-pc-windows-gnu/release/mdedit.exe', 'MDEdit.exe')
    out.write(root / 'LICENSE', 'LICENSE')
    for license_file in sorted((root / 'assets').glob('*LICENSE*')):
        out.write(license_file, 'licenses/' + license_file.name)
digest = hashlib.file_digest(archive.open('rb'), 'sha256').hexdigest()
archive.with_name(archive.name + '.sha256').write_text(f'{digest}  {archive.name}\n')
print(f'Built {archive}')
PY
