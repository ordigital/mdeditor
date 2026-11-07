Noto Color Emoji by Google, unmodified font, SIL Open Font License 1.1.
Upstream: https://github.com/googlefonts/noto-emoji
Source: /usr/share/fonts/noto/NotoColorEmoji.ttf (noto-fonts-emoji system package).
Version: 2.057;GOOG;noto-emoji:20260911:fc4ca365e7c20e78278ae702aa20434bfe704c8f
SHA-256: 15671215ab769fdc7162a045d56fd7d7e477c51b04e6b3c761d914d8fdd6cc44
License: NotoEmoji-LICENSE.txt. Retain it when redistributing this font.

The application embeds the font. Its CBDT PNG glyphs supply the same color
images to the emoji picker, editor and PDF export. No system font is needed
at runtime. Unicode remains the document text. No new rendering dependency
is used: ttf-parser reads glyphs, image decodes PNG, and egui draws textures.
