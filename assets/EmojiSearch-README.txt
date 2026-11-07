Emoji names and search keywords from Unicode CLDR release-48.
Upstream: https://github.com/unicode-org/cldr/tree/release-48/common/annotations
Documentation: https://cldr.unicode.org/translation/characters/short-names-and-keywords
License: Unicode-LICENSE.txt (Unicode License v3), included in packaged distributions.

Regenerate: python3 tools/update_emoji_search.py (network access required).
The script uses Python standard-library XML/gzip support and derives all 23
language codes from src/i18n.rs. It includes single-character emoji annotations,
normalizing optional emoji variation selectors, with localized short names,
search keywords and accent-free spellings. Data are compressed deterministically.
Decompressed TSV columns: character, language code, localized name, search terms.

Runtime uses the current interface language and English. Only emoji supported
by the existing color palette are offered. The index is embedded and inflated
once using flate2, which is already used by the image dependency. Application
builds and searches require no network access or Python installation.
