#!/usr/bin/env python3
"""Refresh compact, offline emoji names/keywords from Unicode CLDR (stdlib only)."""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import re
import gzip
import unicodedata
import urllib.request
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
VERSION = "release-48"
BASE = f"https://raw.githubusercontent.com/unicode-org/cldr/{VERSION}"
LANGUAGES = sorted(set(re.findall(r'Lang::\w+ => "([a-z]{2})"', (ROOT / "src/i18n.rs").read_text())))


def download(path):
    with urllib.request.urlopen(f"{BASE}/{path}", timeout=30) as response:
        return response.read()


def annotations(language):
    xml = ET.fromstring(download(f"common/annotations/{language}.xml"))
    entries = {}
    for annotation in xml.iter("annotation"):
        char = annotation.attrib["cp"].replace("\ufe0f", "")
        if len(char) != 1 or not (0x2300 <= ord(char) <= 0x27BF or 0x1F000 <= ord(char) <= 0x1FAFF):
            continue
        text = " ".join("".join(annotation.itertext()).split())
        if not text or text == "↑↑↑":
            continue
        entry = entries.setdefault(char, {"name": "", "terms": []})
        if annotation.attrib.get("type") == "tts":
            entry["name"] = text
        entry["terms"].append(text)
    rows = []
    for char, entry in sorted(entries.items()):
        if not entry["name"]:
            continue
        words = " ".join(entry["terms"]).replace("|", " ").lower()
        # Include accent-free spellings without a runtime normalization library.
        folded = "".join(c for c in unicodedata.normalize("NFKD", words) if not unicodedata.combining(c))
        folded = folded.translate(str.maketrans({"ł": "l", "ø": "o", "ß": "ss", "đ": "d", "æ": "ae", "œ": "oe"}))
        terms = " ".join(dict.fromkeys((words + " " + folded).split()))
        rows.append(f"{char}\t{language}\t{entry['name']}\t{terms}\n")
    print(f"{language}: {len(rows)} emoji annotations", flush=True)
    return "".join(rows)


if __name__ == "__main__":
    assert len(LANGUAGES) == 23, LANGUAGES
    with ThreadPoolExecutor(max_workers=4) as pool:
        data = "".join(pool.map(annotations, LANGUAGES))
    license_text = download("LICENSE")
    (ROOT / "assets/emoji-search.tsv.gz").write_bytes(gzip.compress(data.encode(), mtime=0))
    (ROOT / "assets/Unicode-LICENSE.txt").write_bytes(license_text)
    print(f"Wrote {len(data.encode()):,} bytes from CLDR {VERSION}")
