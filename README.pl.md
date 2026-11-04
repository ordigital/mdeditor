# MDEdit

Szybki, minimalistyczny **edytor Markdown typu WYSIWYG** napisany w Rust z użyciem [egui](https://github.com/emilk/egui). Działa jak podgląd na żywo w Obsidianie: znaczniki są ukryte wszędzie poza linią, w której stoi kursor, więc `# Tytuł` staje się nagłówkiem, gdy tylko przesuniesz kursor dalej. Tylko ciemny motyw. Interfejs jest dostępny w 23 językach (wybór w Preferencjach).

![MDEdit](docs/screenshot.png)

## Funkcje

- **Podgląd na żywo** nagłówków, pogrubienia, kursywy, przekreślenia, `==wyróżnienia==`, kodu w linii i bloków kodu, linków, `[[linków wiki]]`, `#tagów`, przypisów, `%%komentarzy%%`, linii poziomych i nagłówka YAML.
- **Listy i zadania** z wiszącym wcięciem, klikalne pola wyboru. Enter dodaje kolejny punkt listy, Tab i Shift+Tab zmieniają wcięcie.
- **Cytaty i callouty** (`> [!note]`, `tip`, `warning`, `danger`, `question` itd.) oraz **tabele** z wyrównaniem kolumn. Długi tekst w komórkach zawija się do nowych linii.
- **Wzory**: `$…$`, `\(…\)`, `$$…$$`, `\[…\]`, także `\boxed{}` i `\text{}`, wyświetlane jako Unicode.
- **Obrazy**: z plików, osadzone jako base64 `data:`, `![[embedy]]` i szerokości `![alt|300](…)`. Ctrl+V z obrazem w schowku osadza go jako JPEG (85%, maks. 1200 px szerokości) albo zapisuje WebP obok dokumentu.
- **Karty, szukaj i zamień, cofnij/ponów** (10 kroków), ostatnie pliki, **panel plików** (tworzenie, otwieranie, usuwanie do kosza).
- Skróty formatowania, które także zmieniają zaznaczony tekst (Ctrl+B, Ctrl+I itd.). Ctrl+kółko myszy zmienia rozmiar czcionki.
- **Jedna instancja** (Linux): ponowne uruchomienie `mdedit plik.md` otwiera plik w nowej karcie działającego okna.
- Schowek zaznaczenia w Linuksie: zaznacz tekst, wklej środkowym przyciskiem myszy albo Shift+Insert.
- Domyślne ustawienia z ciemnego motywu Obsidiana: czcionki Inter (tekst) i Source Code Pro (kod), obie wbudowane. Interfejs używa czcionki systemowej (ustawienia KDE / GNOME, zapasowo Noto Sans 10).
- Pliki binarne i bardzo duże (ponad 5 MB) są odrzucane, zamiast zamrażać edytor.
- Końce linii Windows (CRLF) są zachowywane przy zapisie. Ctrl+klik otwiera tylko linki internetowe i pocztowe, nigdy programów ani skryptów.

`examples/example.md` (po angielsku) i `examples/przyklad.md` (po polsku) pokazują wszystkie obsługiwane elementy.

## Budowanie i uruchamianie

Wymagany Rust 1.95 lub nowszy (edycja 2024).

```sh
cargo run --release -- examples/example.md
# albo
cargo build --release && ./target/release/mdedit notatki.md
cargo test --release
```

W Linuksie potrzebne są zwykłe biblioteki okienkowe (Wayland i/lub X11, `libxkbcommon`) oraz, dla okien wyboru plików, portal XDG (KDE, GNOME itd.). Obrazy ze schowka na Wayland korzystają z protokołów `wl-clipboard` wbudowanych w program.

### Wpis w menu i ikona

```sh
./packaging/install.sh
```

instaluje dla bieżącego użytkownika `mdedit.desktop` i ikonę. Na Wayland ikona na pasku zadań pochodzi z tego pliku (dopasowanie po identyfikatorze aplikacji `mdedit`); na X11 i Windows ikona jest też osadzona w oknie.

## Skróty klawiszowe

| Akcja | Klawisze |
| --- | --- |
| Nowy / Otwórz / Zapisz / Zapisz jako | Ctrl+N / Ctrl+O / Ctrl+S / Ctrl+Shift+S |
| Zapisz wszystko | Ctrl+Shift+Alt+S (Ctrl+Shift+Cmd+S na macOS)¹ |
| Zamknij / Zamknij wszystko / Wyjście | Ctrl+W / Ctrl+Shift+W / Ctrl+Q |
| Cofnij / Ponów | Ctrl+Z / Ctrl+Y, Ctrl+Shift+Z |
| Kopiuj / Wklej (tekst lub obraz) | Ctrl+C / Ctrl+V |
| Wklej zaznaczenie (schowek zaznaczenia) | Środkowy przycisk myszy, Shift+Insert |
| Szukaj / Zamień | Ctrl+F |
| Wstaw obraz z pliku | Ctrl+Shift+I |
| Panel plików | Ctrl+Shift+F |
| Preferencje / O programie | Ctrl+, / F1 |
| Pogrubienie / Kursywa / Przekreślenie | Ctrl+B / Ctrl+I / Ctrl+D |
| Kod w linii / Wyróżnienie / Link | Ctrl+E / Ctrl+Shift+H / Ctrl+K |
| Nagłówek 1–6 / zwykły tekst | Ctrl+1 … Ctrl+6 / Ctrl+0 |
| Lista punktowana / zadanie / cytat | Ctrl+Shift+L / Ctrl+L / Ctrl+Shift+Q |
| Otwórz link lub przejdź do przypisu | Ctrl+klik |
| Zmiana rozmiaru czcionki | Ctrl+kółko myszy |
| Wcięcie punktu listy | Tab / Shift+Tab |

¹ egui nie raportuje klawisza Super/Meta w Linuksie i Windows, dlatego zamiast niego używany jest Alt.

## Preferencje

Edycja → Preferencje… (Ctrl+,): czcionka edytora, grubość tekstu / pogrubienia / kodu, gamma tekstu, rozmiar czcionki, interlinia, sposób zapisu wklejanych obrazów i język. **Resetuj** przywraca domyślne ustawienia czcionek (Inter, grubości 300 / 600 / 400, gamma 0,80, 17 px, interlinia 1,65). Widok zawiera przełączniki: szerokość czytelnej linii, numery linii, tytuł w treści i panel plików.

Ustawienia są w `~/.config/mdedit/config`, ostatnie pliki w `~/.config/mdedit/recent`.

## Układ projektu

| Plik | Przeznaczenie |
| --- | --- |
| `src/main.rs` | punkt wejścia, gniazdo jednej instancji, ikona okna |
| `src/app.rs` | aplikacja: menu, karty, edytor, okna dialogowe, cofanie, ustawienia |
| `src/md.rs` | układ Markdown do stylizowanego tekstu i dekoracje |
| `src/latex.rs` | zamiana wzorów LaTeX na Unicode |
| `src/files.rs` | panel plików |
| `src/fonts.rs` | ładowanie czcionek (wbudowane Inter / Source Code Pro, czcionki systemowe przez fontconfig) |
| `src/base64.rs`, `src/i18n.rs` | base64 dla osadzonych obrazów, teksty po angielsku i po polsku |

## Licencja i autorzy

MIT, zob. [LICENSE](LICENSE). Autor: ORDIGITAL Adam Mateusz Brożyński.

Kod napisany z pomocą Claude Opus 5.5 i Claude Sonnet 5.5 (Anthropic).

Czcionki: [Inter](https://github.com/rsms/inter) i [Source Code Pro](https://github.com/adobe-fonts/source-code-pro), obie na licencji SIL Open Font License 1.1 (zob. `assets/`). Kolory i proporcje są zgodne z domyślnym ciemnym motywem Obsidiana; projekt nie jest powiązany z Obsidianem.
