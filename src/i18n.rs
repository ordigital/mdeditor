//! UI languages. English is the source language: a string missing from a language's table is shown in English.
//! Tables are `key|translation` lines; the keys are the English strings used in the code.

use std::collections::HashMap;
use std::sync::OnceLock;

/// In alphabetical order of the English names (the order of the language menu).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Arabic,
    Bengali,
    Chinese,
    Czech,
    English,
    French,
    German,
    Greek,
    Hebrew,
    Hindi,
    Hungarian,
    Italian,
    Japanese,
    Kashubian,
    Lithuanian,
    Polish,
    Portuguese,
    Romanian,
    Russian,
    Slovak,
    Spanish,
    Turkish,
    Ukrainian,
    Urdu,
}

impl Lang {
    pub const ALL: [Lang; 24] = [
        Lang::Arabic,
        Lang::Bengali,
        Lang::Chinese,
        Lang::Czech,
        Lang::English,
        Lang::French,
        Lang::German,
        Lang::Greek,
        Lang::Hebrew,
        Lang::Hindi,
        Lang::Hungarian,
        Lang::Italian,
        Lang::Japanese,
        Lang::Kashubian,
        Lang::Lithuanian,
        Lang::Polish,
        Lang::Portuguese,
        Lang::Romanian,
        Lang::Russian,
        Lang::Slovak,
        Lang::Spanish,
        Lang::Turkish,
        Lang::Ukrainian,
        Lang::Urdu,
    ];

    /// ISO 639-1 code (Kashubian has none: its ISO 639-2 code is `csb`).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Arabic => "ar",
            Lang::Bengali => "bn",
            Lang::Chinese => "zh",
            Lang::Czech => "cs",
            Lang::English => "en",
            Lang::French => "fr",
            Lang::German => "de",
            Lang::Greek => "el",
            Lang::Hebrew => "he",
            Lang::Hindi => "hi",
            Lang::Hungarian => "hu",
            Lang::Italian => "it",
            Lang::Japanese => "ja",
            Lang::Kashubian => "csb",
            Lang::Lithuanian => "lt",
            Lang::Polish => "pl",
            Lang::Portuguese => "pt",
            Lang::Romanian => "ro",
            Lang::Russian => "ru",
            Lang::Slovak => "sk",
            Lang::Spanish => "es",
            Lang::Turkish => "tr",
            Lang::Ukrainian => "uk",
            Lang::Urdu => "ur",
        }
    }

    /// Name in the language itself, for the language menu.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Arabic => "العربية",
            Lang::Bengali => "বাংলা",
            Lang::Chinese => "中文",
            Lang::Czech => "Čeština",
            Lang::English => "English",
            Lang::French => "Français",
            Lang::German => "Deutsch",
            Lang::Greek => "Ελληνικά",
            Lang::Hebrew => "עברית",
            Lang::Hindi => "हिन्दी",
            Lang::Hungarian => "Magyar",
            Lang::Italian => "Italiano",
            Lang::Japanese => "日本語",
            Lang::Kashubian => "Kaszëbsczi",
            Lang::Lithuanian => "Lietuvių",
            Lang::Polish => "Polski",
            Lang::Portuguese => "Português",
            Lang::Romanian => "Română",
            Lang::Russian => "Русский",
            Lang::Slovak => "Slovenčina",
            Lang::Spanish => "Español",
            Lang::Turkish => "Türkçe",
            Lang::Ukrainian => "Українська",
            Lang::Urdu => "اردو",
        }
    }

    /// Language menu label: native name, then the English name.
    pub fn label(self) -> String {
        format!("{} ({:?})", self.native_name(), self)
    }

    /// Languages that write decimals with a comma.
    pub fn decimal_comma(self) -> bool {
        matches!(
            self,
            Lang::Czech
                | Lang::French
                | Lang::German
                | Lang::Greek
                | Lang::Hungarian
                | Lang::Italian
                | Lang::Kashubian
                | Lang::Lithuanian
                | Lang::Polish
                | Lang::Portuguese
                | Lang::Romanian
                | Lang::Russian
                | Lang::Slovak
                | Lang::Spanish
                | Lang::Turkish
                | Lang::Ukrainian
        )
    }

    /// Finds the language of a locale such as `pl_PL.UTF-8` or `zh_CN`.
    pub fn from_code(code: &str) -> Option<Lang> {
        let prefix = code.split(['_', '.', '-', '@']).next()?.to_ascii_lowercase();
        Lang::ALL.into_iter().find(|l| l.code() == prefix)
    }

    /// The language from the environment (`LC_ALL`, `LC_MESSAGES`, `LANG`), English if unknown.
    pub fn from_env() -> Lang {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
            .and_then(|l| Lang::from_code(&l))
            .unwrap_or(Lang::English)
    }
}

/// Returns the translation of an English UI string (or the string itself when it is not translated).
pub fn tr(lang: Lang, s: &'static str) -> &'static str {
    static TABLES: OnceLock<Vec<HashMap<&'static str, &'static str>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        Lang::ALL
            .iter()
            .map(|l| {
                source(*l)
                    .lines()
                    .filter_map(|line| line.split_once('|'))
                    .map(|(k, v)| (k.trim_end(), v.trim()))
                    .collect()
            })
            .collect()
    });
    // `Lang::ALL` is in enum order, so the discriminant indexes the table.
    tables[lang as usize].get(s).copied().unwrap_or(s)
}

fn source(lang: Lang) -> &'static str {
    match lang {
        Lang::Arabic => AR,
        Lang::Bengali => BN,
        Lang::Chinese => ZH,
        Lang::Czech => CS,
        Lang::English => "",
        Lang::French => FR,
        Lang::German => DE,
        Lang::Greek => EL,
        Lang::Hebrew => HE,
        Lang::Hindi => HI,
        Lang::Hungarian => HU,
        Lang::Italian => IT,
        Lang::Japanese => JA,
        Lang::Kashubian => CSB,
        Lang::Lithuanian => LT,
        Lang::Polish => PL,
        Lang::Portuguese => PT,
        Lang::Romanian => RO,
        Lang::Russian => RU,
        Lang::Slovak => SK,
        Lang::Spanish => ES,
        Lang::Turkish => TR,
        Lang::Ukrainian => UK,
        Lang::Urdu => UR,
    }
}

const PL: &str = r#"
File|Plik
Open…|Otwórz…
Save|Zapisz
Save as…|Zapisz jako…
Save all|Zapisz wszystko
Close|Zamknij
Close all|Zamknij wszystko
Edit|Edycja
Copy|Kopiuj
Paste|Wklej
Find/Replace|Znajdź/Zamień
Insert image…|Wstaw obraz…
View|Widok
Optimize|Optymalizuj
Line numbers|Numery linii
Inline title|Tytuł w treści
Language|Język
Preferences|Preferencje
Preferences…|Preferencje…
Recent|Ostatnie
(empty)|(pusto)
Undo|Cofnij
Redo|Ponów
Files|Pliki
The file is too large (over 5 MB)|Plik jest zbyt duży (ponad 5 MB)
The file is not a text file|Plik nie jest plikiem tekstowym
Ctrl+scroll|Ctrl+kółko myszy
Paste selection|Wklej zaznaczenie
Editor font|Czcionka edytora
Font size|Rozmiar czcionki
Text weight|Grubość tekstu
Bold weight|Grubość pogrubienia
Code weight|Grubość kodu
Reset|Resetuj
Exit|Wyjście
Paste text or image|Wklej tekst lub obraz
Middle click|Środkowy przycisk
New list item|Nowy punkt listy
Indent list item|Wcięcie punktu listy
Open link or footnote|Otwórz link lub przypis
Create new|Utwórz nowy
File name:|Nazwa pliku:
Delete…|Usuń…
Delete|Usunąć
Delete file|Usuwanie pliku
Move to trash|Do kosza
Delete permanently|Usuń trwale
Restore default font settings|Przywróć domyślne ustawienia czcionek
Pasted images|Wklejane obrazy
Embed in the document (JPEG)|Osadzaj w dokumencie (JPEG)
Otherwise saved as WebP files next to the document|W przeciwnym razie zapis jako pliki WebP obok dokumentu
Text gamma|Gamma tekstu
Lower values make text bolder and brighter|Niższa wartość daje grubszy i jaśniejszy tekst
Line height|Interlinia
Filter|Filtruj
characters|znaki
words|słowa
links|linki
About|O programie
Untitled|Bez tytułu
Find|Znajdź
Replace|Zamień
Next|Następny
Replace all|Zamień wszystko
No matches|Brak wyników
Unsaved changes|Niezapisane zmiany
Save changes to|Zapisać zmiany w
Don't save|Nie zapisuj
Cancel|Anuluj
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Otwórz plik (Ctrl+O) lub zacznij pisać w nowej karcie (Ctrl+N).
New|Nowy
Keyboard shortcuts|Skróty klawiszowe
Bold|Pogrubienie
Italic|Kursywa
Strikethrough|Przekreślenie
Inline code|Kod w linii
Highlight|Wyróżnienie
Link|Link
Heading 1–6|Nagłówek 1–6
Normal text|Zwykły tekst
Bullet list|Lista punktowana
Task (checkbox)|Zadanie (checkbox)
Quote|Cytat
License|Licencja
Code written with the help of|Kod napisany z pomocą
Save the file first to paste images.|Najpierw zapisz plik, aby wklejać obrazy.
Error|Błąd
"#;

const DE: &str = r#"
File|Datei
Open…|Öffnen…
Save|Speichern
Save as…|Speichern unter…
Save all|Alle speichern
Close|Schließen
Close all|Alle schließen
Edit|Bearbeiten
Copy|Kopieren
Paste|Einfügen
Find/Replace|Suchen/Ersetzen
Insert image…|Bild einfügen…
View|Ansicht
Optimize|Optimieren
Line numbers|Zeilennummern
Inline title|Titel im Text
Language|Sprache
Preferences|Einstellungen
Preferences…|Einstellungen…
Recent|Zuletzt verwendet
(empty)|(leer)
Undo|Rückgängig
Redo|Wiederholen
Files|Dateien
The file is too large (over 5 MB)|Die Datei ist zu groß (über 5 MB)
The file is not a text file|Die Datei ist keine Textdatei
Ctrl+scroll|Strg+Mausrad
Paste selection|Auswahl einfügen
Editor font|Editor-Schrift
Font size|Schriftgröße
Text weight|Textstärke
Bold weight|Fettstärke
Code weight|Code-Stärke
Reset|Zurücksetzen
Exit|Beenden
Paste text or image|Text oder Bild einfügen
Middle click|Mittlere Maustaste
New list item|Neuer Listenpunkt
Indent list item|Listenpunkt einrücken
Open link or footnote|Link oder Fußnote öffnen
Create new|Neu erstellen
File name:|Dateiname:
Delete…|Löschen…
Delete|Löschen
Delete file|Datei löschen
Move to trash|In den Papierkorb
Delete permanently|Endgültig löschen
Restore default font settings|Standard-Schrifteinstellungen wiederherstellen
Pasted images|Eingefügte Bilder
Embed in the document (JPEG)|Im Dokument einbetten (JPEG)
Otherwise saved as WebP files next to the document|Sonst als WebP-Datei neben dem Dokument speichern
Text gamma|Textgamma
Lower values make text bolder and brighter|Kleinere Werte machen den Text fetter und heller
Line height|Zeilenhöhe
Filter|Filter
characters|Zeichen
words|Wörter
links|Links
About|Über
Untitled|Unbenannt
Find|Suchen
Replace|Ersetzen
Next|Weiter
Replace all|Alle ersetzen
No matches|Keine Treffer
Unsaved changes|Nicht gespeicherte Änderungen
Save changes to|Änderungen speichern in
Don't save|Nicht speichern
Cancel|Abbrechen
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Strg+O öffnet eine Datei, Strg+N einen neuen Tab.
New|Neu
Keyboard shortcuts|Tastenkürzel
Bold|Fett
Italic|Kursiv
Strikethrough|Durchgestrichen
Inline code|Code im Text
Highlight|Hervorhebung
Link|Link
Heading 1–6|Überschrift 1–6
Normal text|Normaler Text
Bullet list|Aufzählung
Task (checkbox)|Aufgabe (Kontrollkästchen)
Quote|Zitat
License|Lizenz
Code written with the help of|Code geschrieben mit Hilfe von
Save the file first to paste images.|Speichere die Datei zuerst, um Bilder einzufügen.
Error|Fehler
"#;

const FR: &str = r#"
File|Fichier
Open…|Ouvrir…
Save|Enregistrer
Save as…|Enregistrer sous…
Save all|Tout enregistrer
Close|Fermer
Close all|Tout fermer
Edit|Édition
Copy|Copier
Paste|Coller
Find/Replace|Rechercher/remplacer
Insert image…|Insérer une image…
View|Affichage
Optimize|Optimiser
Line numbers|Numéros de ligne
Inline title|Titre dans le texte
Language|Langue
Preferences|Préférences
Preferences…|Préférences…
Recent|Récents
(empty)|(vide)
Undo|Annuler
Redo|Rétablir
Files|Fichiers
The file is too large (over 5 MB)|Le fichier est trop volumineux (plus de 5 Mo)
The file is not a text file|Le fichier n'est pas un fichier texte
Ctrl+scroll|Ctrl+molette
Paste selection|Coller la sélection
Editor font|Police de l'éditeur
Font size|Taille de police
Text weight|Graisse du texte
Bold weight|Graisse du gras
Code weight|Graisse du code
Reset|Réinitialiser
Exit|Quitter
Paste text or image|Coller du texte ou une image
Middle click|Clic molette
New list item|Nouvel élément de liste
Indent list item|Indenter l'élément de liste
Open link or footnote|Ouvrir le lien ou la note
Create new|Créer
File name:|Nom du fichier :
Delete…|Supprimer…
Delete|Supprimer
Delete file|Supprimer le fichier
Move to trash|Déplacer dans la corbeille
Delete permanently|Supprimer définitivement
Restore default font settings|Rétablir les polices par défaut
Pasted images|Images collées
Embed in the document (JPEG)|Intégrer au document (JPEG)
Otherwise saved as WebP files next to the document|Sinon, enregistrer en WebP à côté du document
Text gamma|Gamma du texte
Lower values make text bolder and brighter|Des valeurs plus basses rendent le texte plus gras et plus lumineux
Line height|Hauteur de ligne
Filter|Filtre
characters|caractères
words|mots
links|liens
About|À propos
Untitled|Sans titre
Find|Rechercher
Replace|Remplacer
Next|Suivant
Replace all|Tout remplacer
No matches|Aucun résultat
Unsaved changes|Modifications non enregistrées
Save changes to|Enregistrer les modifications de
Don't save|Ne pas enregistrer
Cancel|Annuler
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O ouvre un fichier, Ctrl+N un nouvel onglet.
New|Nouveau
Keyboard shortcuts|Raccourcis clavier
Bold|Gras
Italic|Italique
Strikethrough|Barré
Inline code|Code en ligne
Highlight|Surligner
Link|Lien
Heading 1–6|Titre 1–6
Normal text|Texte normal
Bullet list|Liste à puces
Task (checkbox)|Tâche (case à cocher)
Quote|Citation
License|Licence
Code written with the help of|Code écrit avec l'aide de
Save the file first to paste images.|Enregistrez d'abord le fichier pour coller des images.
Error|Erreur
"#;

const RU: &str = r#"
File|Файл
Open…|Открыть…
Save|Сохранить
Save as…|Сохранить как…
Save all|Сохранить все
Close|Закрыть
Close all|Закрыть все
Edit|Правка
Copy|Копировать
Paste|Вставить
Find/Replace|Поиск/замена
Insert image…|Вставить изображение…
View|Вид
Optimize|Оптимальная ширина
Line numbers|Номера строк
Inline title|Заголовок в тексте
Language|Язык
Preferences|Настройки
Preferences…|Настройки…
Recent|Недавние
(empty)|(пусто)
Undo|Отменить
Redo|Повторить
Files|Файлы
The file is too large (over 5 MB)|Файл слишком большой (более 5 МБ)
The file is not a text file|Файл не является текстовым
Ctrl+scroll|Ctrl+колесо мыши
Paste selection|Вставить выделение
Editor font|Шрифт редактора
Font size|Размер шрифта
Text weight|Толщина текста
Bold weight|Толщина полужирного
Code weight|Толщина кода
Reset|Сбросить
Exit|Выход
Paste text or image|Вставить текст или изображение
Middle click|Средняя кнопка мыши
New list item|Новый пункт списка
Indent list item|Отступ пункта списка
Open link or footnote|Открыть ссылку или сноску
Create new|Создать
File name:|Имя файла:
Delete…|Удалить…
Delete|Удалить
Delete file|Удаление файла
Move to trash|В корзину
Delete permanently|Удалить безвозвратно
Restore default font settings|Восстановить шрифты по умолчанию
Pasted images|Вставленные изображения
Embed in the document (JPEG)|Встроить в документ (JPEG)
Otherwise saved as WebP files next to the document|Иначе сохранять как WebP рядом с документом
Text gamma|Гамма текста
Lower values make text bolder and brighter|Меньшие значения делают текст жирнее и ярче
Line height|Высота строки
Filter|Фильтр
characters|символов
words|слов
links|ссылок
About|О программе
Untitled|Без имени
Find|Найти
Replace|Заменить
Next|Далее
Replace all|Заменить все
No matches|Совпадений нет
Unsaved changes|Несохранённые изменения
Save changes to|Сохранить изменения в
Don't save|Не сохранять
Cancel|Отмена
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O открывает файл, Ctrl+N — новую вкладку.
New|Создать
Keyboard shortcuts|Горячие клавиши
Bold|Полужирный
Italic|Курсив
Strikethrough|Зачёркнутый
Inline code|Код в тексте
Highlight|Выделение
Link|Ссылка
Heading 1–6|Заголовок 1–6
Normal text|Обычный текст
Bullet list|Маркированный список
Task (checkbox)|Задача (флажок)
Quote|Цитата
License|Лицензия
Code written with the help of|Код написан с помощью
Save the file first to paste images.|Сначала сохраните файл, чтобы вставлять изображения.
Error|Ошибка
"#;

const UK: &str = r#"
File|Файл
Open…|Відкрити…
Save|Зберегти
Save as…|Зберегти як…
Save all|Зберегти все
Close|Закрити
Close all|Закрити все
Edit|Редагування
Copy|Копіювати
Paste|Вставити
Find/Replace|Пошук/заміна
Insert image…|Вставити зображення…
View|Вигляд
Optimize|Оптимальна ширина
Line numbers|Номери рядків
Inline title|Заголовок у тексті
Language|Мова
Preferences|Налаштування
Preferences…|Налаштування…
Recent|Нещодавні
(empty)|(порожньо)
Undo|Скасувати
Redo|Повторити
Files|Файли
The file is too large (over 5 MB)|Файл завеликий (понад 5 МБ)
The file is not a text file|Файл не є текстовим
Ctrl+scroll|Ctrl+коліщатко миші
Paste selection|Вставити виділене
Editor font|Шрифт редактора
Font size|Розмір шрифту
Text weight|Товщина тексту
Bold weight|Товщина жирного
Code weight|Товщина коду
Reset|Скинути
Exit|Вихід
Paste text or image|Вставити текст або зображення
Middle click|Середня кнопка миші
New list item|Новий пункт списку
Indent list item|Відступ пункту списку
Open link or footnote|Відкрити посилання або примітку
Create new|Створити
File name:|Назва файлу:
Delete…|Видалити…
Delete|Видалити
Delete file|Видалення файлу
Move to trash|У кошик
Delete permanently|Видалити назавжди
Restore default font settings|Відновити типові шрифти
Pasted images|Вставлені зображення
Embed in the document (JPEG)|Вбудувати в документ (JPEG)
Otherwise saved as WebP files next to the document|Інакше зберігати як WebP поруч із документом
Text gamma|Гамма тексту
Lower values make text bolder and brighter|Менші значення роблять текст жирнішим і яскравішим
Line height|Висота рядка
Filter|Фільтр
characters|символів
words|слів
links|посилань
About|Про програму
Untitled|Без назви
Find|Знайти
Replace|Замінити
Next|Далі
Replace all|Замінити все
No matches|Немає збігів
Unsaved changes|Незбережені зміни
Save changes to|Зберегти зміни в
Don't save|Не зберігати
Cancel|Скасувати
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O відкриває файл, Ctrl+N — нову вкладку.
New|Новий
Keyboard shortcuts|Комбінації клавіш
Bold|Жирний
Italic|Курсив
Strikethrough|Закреслений
Inline code|Код у тексті
Highlight|Виділення
Link|Посилання
Heading 1–6|Заголовок 1–6
Normal text|Звичайний текст
Bullet list|Маркований список
Task (checkbox)|Завдання (прапорець)
Quote|Цитата
License|Ліцензія
Code written with the help of|Код написано за допомогою
Save the file first to paste images.|Спочатку збережіть файл, щоб вставляти зображення.
Error|Помилка
"#;

const CS: &str = r#"
File|Soubor
Open…|Otevřít…
Save|Uložit
Save as…|Uložit jako…
Save all|Uložit vše
Close|Zavřít
Close all|Zavřít vše
Edit|Úpravy
Copy|Kopírovat
Paste|Vložit
Find/Replace|Najít/nahradit
Insert image…|Vložit obrázek…
View|Zobrazení
Optimize|Optimalizovat
Line numbers|Čísla řádků
Inline title|Nadpis v textu
Language|Jazyk
Preferences|Předvolby
Preferences…|Předvolby…
Recent|Nedávné
(empty)|(prázdné)
Undo|Zpět
Redo|Znovu
Files|Soubory
The file is too large (over 5 MB)|Soubor je příliš velký (více než 5 MB)
The file is not a text file|Soubor není textový
Ctrl+scroll|Ctrl+kolečko
Paste selection|Vložit výběr
Editor font|Písmo editoru
Font size|Velikost písma
Text weight|Tloušťka textu
Bold weight|Tloušťka tučného
Code weight|Tloušťka kódu
Reset|Obnovit
Exit|Ukončit
Paste text or image|Vložit text nebo obrázek
Middle click|Prostřední tlačítko
New list item|Nová položka seznamu
Indent list item|Odsadit položku seznamu
Open link or footnote|Otevřít odkaz nebo poznámku
Create new|Vytvořit nový
File name:|Název souboru:
Delete…|Smazat…
Delete|Smazat
Delete file|Smazat soubor
Move to trash|Přesunout do koše
Delete permanently|Smazat trvale
Restore default font settings|Obnovit výchozí nastavení písma
Pasted images|Vložené obrázky
Embed in the document (JPEG)|Vložit do dokumentu (JPEG)
Otherwise saved as WebP files next to the document|Jinak uložit jako WebP vedle dokumentu
Text gamma|Gama textu
Lower values make text bolder and brighter|Nižší hodnoty dělají text tučnější a jasnější
Line height|Výška řádku
Filter|Filtr
characters|znaků
words|slov
links|odkazů
About|O aplikaci
Untitled|Bez názvu
Find|Najít
Replace|Nahradit
Next|Další
Replace all|Nahradit vše
No matches|Žádné shody
Unsaved changes|Neuložené změny
Save changes to|Uložit změny v
Don't save|Neukládat
Cancel|Zrušit
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O otevře soubor, Ctrl+N nový tab.
New|Nový
Keyboard shortcuts|Klávesové zkratky
Bold|Tučně
Italic|Kurzíva
Strikethrough|Přeškrtnutí
Inline code|Kód v textu
Highlight|Zvýraznění
Link|Odkaz
Heading 1–6|Nadpis 1–6
Normal text|Normální text
Bullet list|Odrážkový seznam
Task (checkbox)|Úkol (zaškrtávací pole)
Quote|Citace
License|Licence
Code written with the help of|Kód napsán s pomocí
Save the file first to paste images.|Pro vložení obrázků nejprve soubor uložte.
Error|Chyba
"#;

const SK: &str = r#"
File|Súbor
Open…|Otvoriť…
Save|Uložiť
Save as…|Uložiť ako…
Save all|Uložiť všetko
Close|Zavrieť
Close all|Zavrieť všetko
Edit|Úpravy
Copy|Kopírovať
Paste|Vložiť
Find/Replace|Hľadať/nahradiť
Insert image…|Vložiť obrázok…
View|Zobrazenie
Optimize|Optimalizovať
Line numbers|Čísla riadkov
Inline title|Nadpis v texte
Language|Jazyk
Preferences|Predvoľby
Preferences…|Predvoľby…
Recent|Nedávne
(empty)|(prázdne)
Undo|Späť
Redo|Znova
Files|Súbory
The file is too large (over 5 MB)|Súbor je príliš veľký (viac ako 5 MB)
The file is not a text file|Súbor nie je textový
Ctrl+scroll|Ctrl+koliesko
Paste selection|Vložiť výber
Editor font|Písmo editora
Font size|Veľkosť písma
Text weight|Hrúbka textu
Bold weight|Hrúbka tučného
Code weight|Hrúbka kódu
Reset|Obnoviť
Exit|Ukončiť
Paste text or image|Vložiť text alebo obrázok
Middle click|Stredné tlačidlo
New list item|Nová položka zoznamu
Indent list item|Odsadiť položku zoznamu
Open link or footnote|Otvoriť odkaz alebo poznámku
Create new|Vytvoriť nový
File name:|Názov súboru:
Delete…|Odstrániť…
Delete|Odstrániť
Delete file|Odstrániť súbor
Move to trash|Presunúť do koša
Delete permanently|Odstrániť natrvalo
Restore default font settings|Obnoviť predvolené nastavenia písma
Pasted images|Vložené obrázky
Embed in the document (JPEG)|Vložiť do dokumentu (JPEG)
Otherwise saved as WebP files next to the document|Inak uložiť ako WebP vedľa dokumentu
Text gamma|Gama textu
Lower values make text bolder and brighter|Nižšie hodnoty robia text hrubší a jasnejší
Line height|Výška riadku
Filter|Filter
characters|znakov
words|slov
links|odkazov
About|O programe
Untitled|Bez názvu
Find|Hľadať
Replace|Nahradiť
Next|Ďalej
Replace all|Nahradiť všetko
No matches|Žiadne zhody
Unsaved changes|Neuložené zmeny
Save changes to|Uložiť zmeny v
Don't save|Neukladať
Cancel|Zrušiť
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O otvorí súbor, Ctrl+N novú záložku.
New|Nový
Keyboard shortcuts|Klávesové skratky
Bold|Tučné
Italic|Kurzíva
Strikethrough|Preškrtnutie
Inline code|Kód v texte
Highlight|Zvýraznenie
Link|Odkaz
Heading 1–6|Nadpis 1–6
Normal text|Normálny text
Bullet list|Odrážkový zoznam
Task (checkbox)|Úloha (začiarkavacie políčko)
Quote|Citácia
License|Licencia
Code written with the help of|Kód napísaný s pomocou
Save the file first to paste images.|Na vloženie obrázkov najprv uložte súbor.
Error|Chyba
"#;

const ES: &str = r#"
File|Archivo
Open…|Abrir…
Save|Guardar
Save as…|Guardar como…
Save all|Guardar todo
Close|Cerrar
Close all|Cerrar todo
Edit|Editar
Copy|Copiar
Paste|Pegar
Find/Replace|Buscar/reemplazar
Insert image…|Insertar imagen…
View|Ver
Optimize|Optimizar
Line numbers|Números de línea
Inline title|Título en el texto
Language|Idioma
Preferences|Preferencias
Preferences…|Preferencias…
Recent|Recientes
(empty)|(vacío)
Undo|Deshacer
Redo|Rehacer
Files|Archivos
The file is too large (over 5 MB)|El archivo es demasiado grande (más de 5 MB)
The file is not a text file|El archivo no es de texto
Ctrl+scroll|Ctrl+rueda
Paste selection|Pegar selección
Editor font|Fuente del editor
Font size|Tamaño de fuente
Text weight|Grosor del texto
Bold weight|Grosor de la negrita
Code weight|Grosor del código
Reset|Restablecer
Exit|Salir
Paste text or image|Pegar texto o imagen
Middle click|Clic central
New list item|Nuevo elemento de lista
Indent list item|Sangrar elemento de lista
Open link or footnote|Abrir enlace o nota al pie
Create new|Crear nuevo
File name:|Nombre del archivo:
Delete…|Eliminar…
Delete|Eliminar
Delete file|Eliminar archivo
Move to trash|Mover a la papelera
Delete permanently|Eliminar permanentemente
Restore default font settings|Restaurar la configuración de fuente predeterminada
Pasted images|Imágenes pegadas
Embed in the document (JPEG)|Incrustar en el documento (JPEG)
Otherwise saved as WebP files next to the document|Si no, guardar como WebP junto al documento
Text gamma|Gamma del texto
Lower values make text bolder and brighter|Valores más bajos hacen el texto más grueso y brillante
Line height|Altura de línea
Filter|Filtrar
characters|caracteres
words|palabras
links|enlaces
About|Acerca de
Untitled|Sin título
Find|Buscar
Replace|Reemplazar
Next|Siguiente
Replace all|Reemplazar todo
No matches|Sin coincidencias
Unsaved changes|Cambios sin guardar
Save changes to|¿Guardar los cambios en
Don't save|No guardar
Cancel|Cancelar
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O abre un archivo; Ctrl+N inicia una pestaña nueva.
New|Nuevo
Keyboard shortcuts|Atajos de teclado
Bold|Negrita
Italic|Cursiva
Strikethrough|Tachado
Inline code|Código en línea
Highlight|Resaltar
Link|Enlace
Heading 1–6|Encabezado 1–6
Normal text|Texto normal
Bullet list|Lista con viñetas
Task (checkbox)|Tarea (casilla)
Quote|Cita
License|Licencia
Code written with the help of|Código escrito con ayuda de
Save the file first to paste images.|Guarda primero el archivo para pegar imágenes.
Error|Error
"#;

const PT: &str = r#"
File|Arquivo
Open…|Abrir…
Save|Salvar
Save as…|Salvar como…
Save all|Salvar tudo
Close|Fechar
Close all|Fechar tudo
Edit|Editar
Copy|Copiar
Paste|Colar
Find/Replace|Localizar/substituir
Insert image…|Inserir imagem…
View|Exibir
Optimize|Otimizar
Line numbers|Números de linha
Inline title|Título no texto
Language|Idioma
Preferences|Preferências
Preferences…|Preferências…
Recent|Recentes
(empty)|(vazio)
Undo|Desfazer
Redo|Refazer
Files|Arquivos
The file is too large (over 5 MB)|O arquivo é muito grande (mais de 5 MB)
The file is not a text file|O arquivo não é de texto
Ctrl+scroll|Ctrl+rolagem
Paste selection|Colar seleção
Editor font|Fonte do editor
Font size|Tamanho da fonte
Text weight|Espessura do texto
Bold weight|Espessura do negrito
Code weight|Espessura do código
Reset|Redefinir
Exit|Sair
Paste text or image|Colar texto ou imagem
Middle click|Clique com o botão do meio
New list item|Novo item de lista
Indent list item|Recuar item de lista
Open link or footnote|Abrir link ou nota de rodapé
Create new|Criar novo
File name:|Nome do arquivo:
Delete…|Excluir…
Delete|Excluir
Delete file|Excluir arquivo
Move to trash|Mover para a lixeira
Delete permanently|Excluir permanentemente
Restore default font settings|Restaurar as configurações padrão de fonte
Pasted images|Imagens coladas
Embed in the document (JPEG)|Incorporar no documento (JPEG)
Otherwise saved as WebP files next to the document|Caso contrário, salvar como WebP ao lado do documento
Text gamma|Gama do texto
Lower values make text bolder and brighter|Valores menores deixam o texto mais grosso e brilhante
Line height|Altura da linha
Filter|Filtrar
characters|caracteres
words|palavras
links|links
About|Sobre
Untitled|Sem título
Find|Localizar
Replace|Substituir
Next|Próximo
Replace all|Substituir tudo
No matches|Nenhum resultado
Unsaved changes|Alterações não salvas
Save changes to|Salvar as alterações em
Don't save|Não salvar
Cancel|Cancelar
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O abre um arquivo; Ctrl+N inicia uma nova aba.
New|Novo
Keyboard shortcuts|Atalhos de teclado
Bold|Negrito
Italic|Itálico
Strikethrough|Riscado
Inline code|Código em linha
Highlight|Realçar
Link|Link
Heading 1–6|Título 1–6
Normal text|Texto normal
Bullet list|Lista com marcadores
Task (checkbox)|Tarefa (caixa de seleção)
Quote|Citação
License|Licença
Code written with the help of|Código escrito com ajuda de
Save the file first to paste images.|Salve o arquivo primeiro para colar imagens.
Error|Erro
"#;

const IT: &str = r#"
File|File
Open…|Apri…
Save|Salva
Save as…|Salva con nome…
Save all|Salva tutto
Close|Chiudi
Close all|Chiudi tutto
Edit|Modifica
Copy|Copia
Paste|Incolla
Find/Replace|Trova/sostituisci
Insert image…|Inserisci immagine…
View|Visualizza
Optimize|Ottimizza
Line numbers|Numeri di riga
Inline title|Titolo nel testo
Language|Lingua
Preferences|Preferenze
Preferences…|Preferenze…
Recent|Recenti
(empty)|(vuoto)
Undo|Annulla
Redo|Ripeti
Files|File
The file is too large (over 5 MB)|Il file è troppo grande (oltre 5 MB)
The file is not a text file|Il file non è di testo
Ctrl+scroll|Ctrl+rotellina
Paste selection|Incolla selezione
Editor font|Carattere dell'editor
Font size|Dimensione carattere
Text weight|Spessore del testo
Bold weight|Spessore del grassetto
Code weight|Spessore del codice
Reset|Ripristina
Exit|Esci
Paste text or image|Incolla testo o immagine
Middle click|Clic con il pulsante centrale
New list item|Nuovo elemento di elenco
Indent list item|Rientra elemento di elenco
Open link or footnote|Apri link o nota
Create new|Crea nuovo
File name:|Nome file:
Delete…|Elimina…
Delete|Elimina
Delete file|Elimina file
Move to trash|Sposta nel cestino
Delete permanently|Elimina definitivamente
Restore default font settings|Ripristina i caratteri predefiniti
Pasted images|Immagini incollate
Embed in the document (JPEG)|Incorpora nel documento (JPEG)
Otherwise saved as WebP files next to the document|Altrimenti salva come WebP accanto al documento
Text gamma|Gamma del testo
Lower values make text bolder and brighter|Valori più bassi rendono il testo più spesso e luminoso
Line height|Altezza riga
Filter|Filtro
characters|caratteri
words|parole
links|link
About|Info
Untitled|Senza titolo
Find|Trova
Replace|Sostituisci
Next|Successivo
Replace all|Sostituisci tutto
No matches|Nessun risultato
Unsaved changes|Modifiche non salvate
Save changes to|Salvare le modifiche a
Don't save|Non salvare
Cancel|Annulla
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O apre un file, Ctrl+N una nuova scheda.
New|Nuovo
Keyboard shortcuts|Scorciatoie da tastiera
Bold|Grassetto
Italic|Corsivo
Strikethrough|Barrato
Inline code|Codice in linea
Highlight|Evidenzia
Link|Collegamento
Heading 1–6|Titolo 1–6
Normal text|Testo normale
Bullet list|Elenco puntato
Task (checkbox)|Attività (casella)
Quote|Citazione
License|Licenza
Code written with the help of|Codice scritto con l'aiuto di
Save the file first to paste images.|Salva prima il file per incollare immagini.
Error|Errore
"#;

const HU: &str = r#"
File|Fájl
Open…|Megnyitás…
Save|Mentés
Save as…|Mentés másként…
Save all|Összes mentése
Close|Bezárás
Close all|Összes bezárása
Edit|Szerkesztés
Copy|Másolás
Paste|Beillesztés
Find/Replace|Keresés/csere
Insert image…|Kép beszúrása…
View|Nézet
Optimize|Optimalizálás
Line numbers|Sorszámok
Inline title|Cím a szövegben
Language|Nyelv
Preferences|Beállítások
Preferences…|Beállítások…
Recent|Legutóbbiak
(empty)|(üres)
Undo|Visszavonás
Redo|Újra
Files|Fájlok
The file is too large (over 5 MB)|A fájl túl nagy (5 MB felett)
The file is not a text file|A fájl nem szövegfájl
Ctrl+scroll|Ctrl+görgetés
Paste selection|Kijelölés beillesztése
Editor font|Szerkesztő betűtípusa
Font size|Betűméret
Text weight|Szöveg vastagsága
Bold weight|Félkövér vastagsága
Code weight|Kód vastagsága
Reset|Visszaállítás
Exit|Kilépés
Paste text or image|Szöveg vagy kép beillesztése
Middle click|Középső egérgomb
New list item|Új listaelem
Indent list item|Listaelem behúzása
Open link or footnote|Hivatkozás vagy lábjegyzet megnyitása
Create new|Új létrehozása
File name:|Fájlnév:
Delete…|Törlés…
Delete|Törlés
Delete file|Fájl törlése
Move to trash|Áthelyezés a kukába
Delete permanently|Végleges törlés
Restore default font settings|Alapértelmezett betűbeállítások visszaállítása
Pasted images|Beillesztett képek
Embed in the document (JPEG)|Beágyazás a dokumentumba (JPEG)
Otherwise saved as WebP files next to the document|Egyébként WebP fájlként a dokumentum mellé mentve
Text gamma|Szöveg gamma
Lower values make text bolder and brighter|Az alacsonyabb értékek vastagabbá és világosabbá teszik a szöveget
Line height|Sormagasság
Filter|Szűrő
characters|karakter
words|szó
links|hivatkozás
About|Névjegy
Untitled|Névtelen
Find|Keresés
Replace|Csere
Next|Következő
Replace all|Összes cseréje
No matches|Nincs találat
Unsaved changes|Nem mentett módosítások
Save changes to|Módosítások mentése ebbe:
Don't save|Ne mentse
Cancel|Mégse
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O fájlt nyit, Ctrl+N új lapon kezdhet gépelni.
New|Új
Keyboard shortcuts|Billentyűparancsok
Bold|Félkövér
Italic|Dőlt
Strikethrough|Áthúzott
Inline code|Kód a szövegben
Highlight|Kiemelés
Link|Hivatkozás
Heading 1–6|Címsor 1–6
Normal text|Normál szöveg
Bullet list|Felsorolás
Task (checkbox)|Feladat (jelölőnégyzet)
Quote|Idézet
License|Licenc
Code written with the help of|A kódot segítségével írta:
Save the file first to paste images.|A képek beillesztéséhez először mentse a fájlt.
Error|Hiba
"#;

const TR: &str = r#"
File|Dosya
Open…|Aç…
Save|Kaydet
Save as…|Farklı kaydet…
Save all|Tümünü kaydet
Close|Kapat
Close all|Tümünü kapat
Edit|Düzen
Copy|Kopyala
Paste|Yapıştır
Find/Replace|Bul/değiştir
Insert image…|Resim ekle…
View|Görünüm
Optimize|Optimize et
Line numbers|Satır numaraları
Inline title|Metin içi başlık
Language|Dil
Preferences|Tercihler
Preferences…|Tercihler…
Recent|Son kullanılanlar
(empty)|(boş)
Undo|Geri al
Redo|Yinele
Files|Dosyalar
The file is too large (over 5 MB)|Dosya çok büyük (5 MB'dan fazla)
The file is not a text file|Dosya metin dosyası değil
Ctrl+scroll|Ctrl+kaydırma
Paste selection|Seçimi yapıştır
Editor font|Düzenleyici yazı tipi
Font size|Yazı boyutu
Text weight|Metin kalınlığı
Bold weight|Kalın kalınlığı
Code weight|Kod kalınlığı
Reset|Sıfırla
Exit|Çıkış
Paste text or image|Metin veya resim yapıştır
Middle click|Orta tık
New list item|Yeni liste öğesi
Indent list item|Liste öğesini girintile
Open link or footnote|Bağlantıyı veya dipnotu aç
Create new|Yeni oluştur
File name:|Dosya adı:
Delete…|Sil…
Delete|Sil
Delete file|Dosyayı sil
Move to trash|Çöp kutusuna taşı
Delete permanently|Kalıcı olarak sil
Restore default font settings|Varsayılan yazı tipi ayarlarını geri yükle
Pasted images|Yapıştırılan resimler
Embed in the document (JPEG)|Belgeye göm (JPEG)
Otherwise saved as WebP files next to the document|Aksi halde belgenin yanına WebP olarak kaydet
Text gamma|Metin gamma
Lower values make text bolder and brighter|Düşük değerler metni daha kalın ve parlak yapar
Line height|Satır yüksekliği
Filter|Filtre
characters|karakter
words|kelime
links|bağlantı
About|Hakkında
Untitled|Adsız
Find|Bul
Replace|Değiştir
Next|Sonraki
Replace all|Tümünü değiştir
No matches|Eşleşme yok
Unsaved changes|Kaydedilmemiş değişiklikler
Save changes to|Değişiklikleri kaydet:
Don't save|Kaydetme
Cancel|İptal
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O ile dosya aç, Ctrl+N ile yeni sekmede yazmaya başla.
New|Yeni
Keyboard shortcuts|Klavye kısayolları
Bold|Kalın
Italic|İtalik
Strikethrough|Üstü çizili
Inline code|Satır içi kod
Highlight|Vurgula
Link|Bağlantı
Heading 1–6|Başlık 1–6
Normal text|Normal metin
Bullet list|Madde işaretli liste
Task (checkbox)|Görev (onay kutusu)
Quote|Alıntı
License|Lisans
Code written with the help of|Kod şunun yardımıyla yazıldı:
Save the file first to paste images.|Resim yapıştırmak için önce dosyayı kaydedin.
Error|Hata
"#;

const RO: &str = r#"
File|Fișier
Open…|Deschide…
Save|Salvează
Save as…|Salvează ca…
Save all|Salvează tot
Close|Închide
Close all|Închide tot
Edit|Editare
Copy|Copiază
Paste|Lipește
Find/Replace|Caută/înlocuiește
Insert image…|Inserează imagine…
View|Vizualizare
Optimize|Optimizează
Line numbers|Numere de rând
Inline title|Titlu în text
Language|Limbă
Preferences|Preferințe
Preferences…|Preferințe…
Recent|Recente
(empty)|(gol)
Undo|Anulează
Redo|Refă
Files|Fișiere
The file is too large (over 5 MB)|Fișierul este prea mare (peste 5 MB)
The file is not a text file|Fișierul nu este unul text
Ctrl+scroll|Ctrl+derulare
Paste selection|Lipește selecția
Editor font|Fontul editorului
Font size|Dimensiunea fontului
Text weight|Grosimea textului
Bold weight|Grosimea aldinei
Code weight|Grosimea codului
Reset|Resetează
Exit|Ieșire
Paste text or image|Lipește text sau imagine
Middle click|Clic cu mijlocul
New list item|Element nou de listă
Indent list item|Indentează elementul de listă
Open link or footnote|Deschide link-ul sau nota de subsol
Create new|Creează nou
File name:|Numele fișierului:
Delete…|Șterge…
Delete|Șterge
Delete file|Șterge fișierul
Move to trash|Mută în coșul de gunoi
Delete permanently|Șterge definitiv
Restore default font settings|Restabilește setările implicite ale fontului
Pasted images|Imagini lipite
Embed in the document (JPEG)|Încorporează în document (JPEG)
Otherwise saved as WebP files next to the document|Altfel salvează ca WebP lângă document
Text gamma|Gama textului
Lower values make text bolder and brighter|Valorile mai mici fac textul mai gros și mai luminos
Line height|Înălțimea rândului
Filter|Filtru
characters|caractere
words|cuvinte
links|link-uri
About|Despre
Untitled|Fără nume
Find|Caută
Replace|Înlocuiește
Next|Următorul
Replace all|Înlocuiește tot
No matches|Niciun rezultat
Unsaved changes|Modificări nesalvate
Save changes to|Salvezi modificările în
Don't save|Nu salva
Cancel|Anulează
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O deschide un fișier, Ctrl+N un tab nou.
New|Nou
Keyboard shortcuts|Scurtături de la tastatură
Bold|Aldine
Italic|Cursiv
Strikethrough|Tăiat
Inline code|Cod în text
Highlight|Evidențiere
Link|Link
Heading 1–6|Titlu 1–6
Normal text|Text normal
Bullet list|Listă cu marcatori
Task (checkbox)|Sarcină (casetă de bifare)
Quote|Citat
License|Licență
Code written with the help of|Cod scris cu ajutorul
Save the file first to paste images.|Salvează mai întâi fișierul pentru a lipi imagini.
Error|Eroare
"#;

const EL: &str = r#"
File|Αρχείο
Open…|Άνοιγμα…
Save|Αποθήκευση
Save as…|Αποθήκευση ως…
Save all|Αποθήκευση όλων
Close|Κλείσιμο
Close all|Κλείσιμο όλων
Edit|Επεξεργασία
Copy|Αντιγραφή
Paste|Επικόλληση
Find/Replace|Εύρεση/αντικατάσταση
Insert image…|Εισαγωγή εικόνας…
View|Προβολή
Optimize|Βελτιστοποίηση
Line numbers|Αριθμοί γραμμών
Inline title|Τίτλος στο κείμενο
Language|Γλώσσα
Preferences|Προτιμήσεις
Preferences…|Προτιμήσεις…
Recent|Πρόσφατα
(empty)|(κενό)
Undo|Αναίρεση
Redo|Επανάληψη
Files|Αρχεία
The file is too large (over 5 MB)|Το αρχείο είναι πολύ μεγάλο (άνω των 5 MB)
The file is not a text file|Το αρχείο δεν είναι αρχείο κειμένου
Ctrl+scroll|Ctrl+κύλιση
Paste selection|Επικόλληση επιλογής
Editor font|Γραμματοσειρά επεξεργαστή
Font size|Μέγεθος γραμματοσειράς
Text weight|Πάχος κειμένου
Bold weight|Πάχος έντονου
Code weight|Πάχος κώδικα
Reset|Επαναφορά
Exit|Έξοδος
Paste text or image|Επικόλληση κειμένου ή εικόνας
Middle click|Μεσαίο κλικ
New list item|Νέο στοιχείο λίστας
Indent list item|Εσοχή στοιχείου λίστας
Open link or footnote|Άνοιγμα συνδέσμου ή υποσημείωσης
Create new|Δημιουργία νέου
File name:|Όνομα αρχείου:
Delete…|Διαγραφή…
Delete|Διαγραφή
Delete file|Διαγραφή αρχείου
Move to trash|Μετακίνηση στον κάδο
Delete permanently|Οριστική διαγραφή
Restore default font settings|Επαναφορά προεπιλεγμένων ρυθμίσεων γραμματοσειράς
Pasted images|Επικολλημένες εικόνες
Embed in the document (JPEG)|Ενσωμάτωση στο έγγραφο (JPEG)
Otherwise saved as WebP files next to the document|Διαφορετικά αποθήκευση ως WebP δίπλα στο έγγραφο
Text gamma|Γάμμα κειμένου
Lower values make text bolder and brighter|Οι μικρότερες τιμές κάνουν το κείμενο πιο έντονο και φωτεινό
Line height|Ύψος γραμμής
Filter|Φίλτρο
characters|χαρακτήρες
words|λέξεις
links|σύνδεσμοι
About|Σχετικά
Untitled|Χωρίς τίτλο
Find|Εύρεση
Replace|Αντικατάσταση
Next|Επόμενο
Replace all|Αντικατάσταση όλων
No matches|Δεν βρέθηκαν αποτελέσματα
Unsaved changes|Μη αποθηκευμένες αλλαγές
Save changes to|Αποθήκευση αλλαγών σε
Don't save|Χωρίς αποθήκευση
Cancel|Ακύρωση
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Το Ctrl+O ανοίγει αρχείο, το Ctrl+N νέα καρτέλα.
New|Νέο
Keyboard shortcuts|Συντομεύσεις πληκτρολογίου
Bold|Έντονα
Italic|Πλάγια
Strikethrough|Διαγραμμένο
Inline code|Κώδικας στη γραμμή
Highlight|Επισήμανση
Link|Σύνδεσμος
Heading 1–6|Επικεφαλίδα 1–6
Normal text|Κανονικό κείμενο
Bullet list|Λίστα με κουκκίδες
Task (checkbox)|Εργασία (πλαίσιο ελέγχου)
Quote|Παράθεση
License|Άδεια
Code written with the help of|Ο κώδικας γράφτηκε με τη βοήθεια
Save the file first to paste images.|Αποθηκεύστε πρώτα το αρχείο για να επικολλήσετε εικόνες.
Error|Σφάλμα
"#;

const LT: &str = r#"
File|Failas
Open…|Atidaryti…
Save|Įrašyti
Save as…|Įrašyti kaip…
Save all|Įrašyti visus
Close|Uždaryti
Close all|Uždaryti visus
Edit|Redagavimas
Copy|Kopijuoti
Paste|Įklijuoti
Find/Replace|Ieškoti/keisti
Insert image…|Įterpti paveikslėlį…
View|Rodinys
Optimize|Optimizuoti
Line numbers|Eilučių numeriai
Inline title|Pavadinimas tekste
Language|Kalba
Preferences|Nuostatos
Preferences…|Nuostatos…
Recent|Naujausi
(empty)|(tuščia)
Undo|Atšaukti
Redo|Pakartoti
Files|Failai
The file is too large (over 5 MB)|Failas per didelis (daugiau nei 5 MB)
The file is not a text file|Failas nėra teksto failas
Ctrl+scroll|Ctrl+ratukas
Paste selection|Įklijuoti pažymėtą
Editor font|Redaktoriaus šriftas
Font size|Šrifto dydis
Text weight|Teksto storis
Bold weight|Pusjuodžio storis
Code weight|Kodo storis
Reset|Atkurti
Exit|Išeiti
Paste text or image|Įklijuoti tekstą ar paveikslėlį
Middle click|Vidurinis pelės mygtukas
New list item|Naujas sąrašo elementas
Indent list item|Įtraukti sąrašo elementą
Open link or footnote|Atidaryti nuorodą ar išnašą
Create new|Sukurti naują
File name:|Failo pavadinimas:
Delete…|Ištrinti…
Delete|Ištrinti
Delete file|Ištrinti failą
Move to trash|Perkelti į šiukšliadėžę
Delete permanently|Ištrinti visam laikui
Restore default font settings|Atkurti numatytuosius šrifto nustatymus
Pasted images|Įklijuoti paveikslėliai
Embed in the document (JPEG)|Įterpti į dokumentą (JPEG)
Otherwise saved as WebP files next to the document|Kitaip įrašyti kaip WebP failą šalia dokumento
Text gamma|Teksto gama
Lower values make text bolder and brighter|Mažesnės reikšmės daro tekstą storesnį ir ryškesnį
Line height|Eilutės aukštis
Filter|Filtras
characters|simbolių
words|žodžių
links|nuorodų
About|Apie
Untitled|Be pavadinimo
Find|Ieškoti
Replace|Keisti
Next|Kitas
Replace all|Keisti visus
No matches|Nerasta
Unsaved changes|Neįrašyti pakeitimai
Save changes to|Įrašyti pakeitimus į
Don't save|Neįrašyti
Cancel|Atšaukti
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O atidaro failą, Ctrl+N — naują skirtuką.
New|Naujas
Keyboard shortcuts|Klavišų trumpiniai
Bold|Pusjuodis
Italic|Kursyvas
Strikethrough|Perbrauktas
Inline code|Kodas tekste
Highlight|Paryškinimas
Link|Nuoroda
Heading 1–6|Antraštė 1–6
Normal text|Įprastas tekstas
Bullet list|Ženklelių sąrašas
Task (checkbox)|Užduotis (žymimasis langelis)
Quote|Citata
License|Licencija
Code written with the help of|Kodas parašytas su pagalba
Save the file first to paste images.|Pirma įrašykite failą, kad galėtumėte įklijuoti paveikslėlius.
Error|Klaida
"#;

const CSB: &str = r#"
File|Plik
Files|Pliczi
Find|Szëkac
Next|Dalszi
Cancel|Anulowac
Error|Bléd
Language|Jãzëk
Recent|Ostatnié
Untitled|Bez miona
"#;

const JA: &str = r#"
File|ファイル
Open…|開く…
Save|保存
Save as…|名前を付けて保存…
Save all|すべて保存
Close|閉じる
Close all|すべて閉じる
Edit|編集
Copy|コピー
Paste|貼り付け
Find/Replace|検索/置換
Insert image…|画像を挿入…
View|表示
Optimize|読みやすい幅
Line numbers|行番号
Inline title|本文内タイトル
Language|言語
Preferences|環境設定
Preferences…|環境設定…
Recent|最近使った項目
(empty)|(なし)
Undo|元に戻す
Redo|やり直し
Files|ファイル
The file is too large (over 5 MB)|ファイルが大きすぎます（5 MB を超えています）
The file is not a text file|テキストファイルではありません
Ctrl+scroll|Ctrl+ホイール
Paste selection|選択範囲を貼り付け
Editor font|エディタのフォント
Font size|フォントサイズ
Text weight|本文の太さ
Bold weight|太字の太さ
Code weight|コードの太さ
Reset|リセット
Exit|終了
Paste text or image|テキストまたは画像を貼り付け
Middle click|中クリック
New list item|新しいリスト項目
Indent list item|リスト項目をインデント
Open link or footnote|リンクまたは脚注を開く
Create new|新規作成
File name:|ファイル名:
Delete…|削除…
Delete|削除
Delete file|ファイルを削除
Move to trash|ゴミ箱に移動
Delete permanently|完全に削除
Restore default font settings|既定のフォント設定に戻す
Pasted images|貼り付けた画像
Embed in the document (JPEG)|文書に埋め込む (JPEG)
Otherwise saved as WebP files next to the document|それ以外は文書と同じフォルダに WebP として保存
Text gamma|本文のガンマ
Lower values make text bolder and brighter|値を小さくすると文字が太く明るくなります
Line height|行の高さ
Filter|絞り込み
characters|文字
words|語
links|リンク
About|バージョン情報
Untitled|無題
Find|検索
Replace|置換
Next|次へ
Replace all|すべて置換
No matches|一致なし
Unsaved changes|未保存の変更
Save changes to|次の変更を保存しますか:
Don't save|保存しない
Cancel|キャンセル
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O でファイルを開くか、Ctrl+N で新しいタブに入力を始めます。
New|新規
Keyboard shortcuts|キーボードショートカット
Bold|太字
Italic|斜体
Strikethrough|取り消し線
Inline code|インラインコード
Highlight|強調表示
Link|リンク
Heading 1–6|見出し 1–6
Normal text|本文
Bullet list|箇条書き
Task (checkbox)|タスク（チェックボックス）
Quote|引用
License|ライセンス
Code written with the help of|コードの作成支援:
Save the file first to paste images.|画像を貼り付けるには先にファイルを保存してください。
Error|エラー
"#;

const ZH: &str = r#"
File|文件
Open…|打开…
Save|保存
Save as…|另存为…
Save all|全部保存
Close|关闭
Close all|全部关闭
Edit|编辑
Copy|复制
Paste|粘贴
Find/Replace|查找/替换
Insert image…|插入图片…
View|视图
Optimize|优化阅读宽度
Line numbers|行号
Inline title|页内标题
Language|语言
Preferences|偏好设置
Preferences…|偏好设置…
Recent|最近打开
(empty)|(空)
Undo|撤销
Redo|重做
Files|文件
The file is too large (over 5 MB)|文件过大（超过 5 MB）
The file is not a text file|不是文本文件
Ctrl+scroll|Ctrl+滚轮
Paste selection|粘贴选中内容
Editor font|编辑器字体
Font size|字号
Text weight|正文字重
Bold weight|粗体字重
Code weight|代码字重
Reset|恢复默认
Exit|退出
Paste text or image|粘贴文本或图片
Middle click|鼠标中键
New list item|新建列表项
Indent list item|缩进列表项
Open link or footnote|打开链接或脚注
Create new|新建
File name:|文件名：
Delete…|删除…
Delete|删除
Delete file|删除文件
Move to trash|移到回收站
Delete permanently|永久删除
Restore default font settings|恢复默认字体设置
Pasted images|粘贴的图片
Embed in the document (JPEG)|嵌入文档（JPEG）
Otherwise saved as WebP files next to the document|否则保存为文档旁的 WebP 文件
Text gamma|文字 gamma
Lower values make text bolder and brighter|较小的值使文字更粗更亮
Line height|行高
Filter|筛选
characters|字符
words|词
links|链接
About|关于
Untitled|未命名
Find|查找
Replace|替换
Next|下一个
Replace all|全部替换
No matches|无匹配结果
Unsaved changes|未保存的更改
Save changes to|是否保存对以下文件的更改：
Don't save|不保存
Cancel|取消
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|按 Ctrl+O 打开文件，或按 Ctrl+N 新建标签页开始输入。
New|新建
Keyboard shortcuts|键盘快捷键
Bold|粗体
Italic|斜体
Strikethrough|删除线
Inline code|行内代码
Highlight|高亮
Link|链接
Heading 1–6|标题 1–6
Normal text|正文
Bullet list|无序列表
Task (checkbox)|任务（复选框）
Quote|引用
License|许可证
Code written with the help of|代码编写协助：
Save the file first to paste images.|请先保存文件，然后再粘贴图片。
Error|错误
"#;

const HE: &str = r#"
File|קובץ
Open…|פתיחה…
Save|שמירה
Save as…|שמירה בשם…
Save all|שמירת הכול
Close|סגירה
Close all|סגירת הכול
Edit|עריכה
Copy|העתקה
Paste|הדבקה
Find/Replace|חיפוש/החלפה
Insert image…|הוספת תמונה…
View|תצוגה
Optimize|התאמה לקריאה
Line numbers|מספרי שורות
Inline title|כותרת בתוך הטקסט
Language|שפה
Preferences|העדפות
Preferences…|העדפות…
Recent|אחרונים
(empty)|(ריק)
Undo|ביטול
Redo|שחזור
Files|קבצים
The file is too large (over 5 MB)|הקובץ גדול מדי (מעל 5 MB)
The file is not a text file|הקובץ אינו קובץ טקסט
Ctrl+scroll|Ctrl+גלילה
Paste selection|הדבקת הבחירה
Editor font|גופן העורך
Font size|גודל גופן
Text weight|עובי טקסט
Bold weight|עובי מודגש
Code weight|עובי קוד
Reset|איפוס
Exit|יציאה
Paste text or image|הדבקת טקסט או תמונה
Middle click|לחיצה אמצעית
New list item|פריט רשימה חדש
Indent list item|הזחת פריט רשימה
Open link or footnote|פתיחת קישור או הערת שוליים
Create new|יצירה חדשה
File name:|שם הקובץ:
Delete…|מחיקה…
Delete|מחיקה
Delete file|מחיקת קובץ
Move to trash|העברה לאשפה
Delete permanently|מחיקה לצמיתות
Restore default font settings|שחזור הגדרות הגופן ברירת המחדל
Pasted images|תמונות שהודבקו
Embed in the document (JPEG)|הטמעה במסמך (JPEG)
Otherwise saved as WebP files next to the document|אחרת נשמרות כקבצי WebP לצד המסמך
Text gamma|גמא טקסט
Lower values make text bolder and brighter|ערכים נמוכים יותר הופכים את הטקסט לעבה ובהיר יותר
Line height|גובה שורה
Filter|סינון
characters|תווים
words|מילים
links|קישורים
About|אודות
Untitled|ללא שם
Find|חיפוש
Replace|החלפה
Next|הבא
Replace all|החלף הכול
No matches|אין תוצאות
Unsaved changes|שינויים שלא נשמרו
Save changes to|לשמור את השינויים ב-
Don't save|לא לשמור
Cancel|ביטול
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O פותח קובץ, Ctrl+N פותח לשונית חדשה.
New|חדש
Keyboard shortcuts|קיצורי מקלדת
Bold|מודגש
Italic|נטוי
Strikethrough|קו חוצה
Inline code|קוד בתוך הטקסט
Highlight|הדגשה
Link|קישור
Heading 1–6|כותרת 1–6
Normal text|טקסט רגיל
Bullet list|רשימת נקודות
Task (checkbox)|משימה (תיבת סימון)
Quote|ציטוט
License|רישיון
Code written with the help of|קוד שנכתב בעזרת
Save the file first to paste images.|שמור את הקובץ קודם כדי להדביק תמונות.
Error|שגיאה
"#;

const HI: &str = r#"
File|फ़ाइल
Open…|खोलें…
Save|सहेजें
Save as…|इस रूप में सहेजें…
Save all|सब सहेजें
Close|बंद करें
Close all|सब बंद करें
Edit|संपादित करें
Copy|कॉपी करें
Paste|चिपकाएँ
Find/Replace|खोजें/बदलें
Insert image…|चित्र डालें…
View|दृश्य
Optimize|अनुकूलित करें
Line numbers|पंक्ति संख्याएँ
Inline title|पाठ में शीर्षक
Language|भाषा
Preferences|प्राथमिकताएँ
Preferences…|प्राथमिकताएँ…
Recent|हाल की फ़ाइलें
(empty)|(खाली)
Undo|पूर्ववत करें
Redo|फिर से करें
Files|फ़ाइलें
The file is too large (over 5 MB)|फ़ाइल बहुत बड़ी है (5 MB से अधिक)
The file is not a text file|यह पाठ फ़ाइल नहीं है
Ctrl+scroll|Ctrl+स्क्रॉल
Paste selection|चयन चिपकाएँ
Editor font|संपादक फ़ॉन्ट
Font size|फ़ॉन्ट आकार
Text weight|पाठ की मोटाई
Bold weight|गाढ़े की मोटाई
Code weight|कोड की मोटाई
Reset|रीसेट करें
Exit|बाहर निकलें
Paste text or image|पाठ या चित्र चिपकाएँ
Middle click|बीच का बटन
New list item|नई सूची मद
Indent list item|सूची मद इंडेंट करें
Open link or footnote|लिंक या पाद-टिप्पणी खोलें
Create new|नया बनाएँ
File name:|फ़ाइल का नाम:
Delete…|हटाएँ…
Delete|हटाएँ
Delete file|फ़ाइल हटाएँ
Move to trash|कूड़ेदान में ले जाएँ
Delete permanently|स्थायी रूप से हटाएँ
Restore default font settings|डिफ़ॉल्ट फ़ॉन्ट सेटिंग्स बहाल करें
Pasted images|चिपकाए गए चित्र
Embed in the document (JPEG)|दस्तावेज़ में एम्बेड करें (JPEG)
Otherwise saved as WebP files next to the document|अन्यथा दस्तावेज़ के बगल में WebP फ़ाइल के रूप में सहेजें
Text gamma|पाठ गामा
Lower values make text bolder and brighter|कम मान पाठ को गाढ़ा और चमकीला बनाते हैं
Line height|पंक्ति की ऊँचाई
Filter|फ़िल्टर
characters|अक्षर
words|शब्द
links|लिंक
About|परिचय
Untitled|शीर्षकहीन
Find|खोजें
Replace|बदलें
Next|अगला
Replace all|सब बदलें
No matches|कोई मिलान नहीं
Unsaved changes|असहेजे बदलाव
Save changes to|इसमें बदलाव सहेजें
Don't save|सहेजें नहीं
Cancel|रद्द करें
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O से फ़ाइल खोलें या Ctrl+N से नए टैब में लिखना शुरू करें।
New|नया
Keyboard shortcuts|कीबोर्ड शॉर्टकट
Bold|बोल्ड
Italic|इटैलिक
Strikethrough|आर-पार रेखा
Inline code|पंक्ति में कोड
Highlight|हाइलाइट
Link|लिंक
Heading 1–6|शीर्षक 1–6
Normal text|सामान्य पाठ
Bullet list|बुलेट सूची
Task (checkbox)|कार्य (चेकबॉक्स)
Quote|उद्धरण
License|लाइसेंस
Code written with the help of|कोड इनकी सहायता से लिखा गया
Save the file first to paste images.|चित्र चिपकाने से पहले फ़ाइल सहेजें।
Error|त्रुटि
"#;

const BN: &str = r#"
File|ফাইল
Open…|খুলুন…
Save|সংরক্ষণ
Save as…|অন্য নামে সংরক্ষণ…
Save all|সব সংরক্ষণ
Close|বন্ধ করুন
Close all|সব বন্ধ করুন
Edit|সম্পাদনা
Copy|কপি
Paste|পেস্ট
Find/Replace|খুঁজুন/প্রতিস্থাপন
Insert image…|ছবি সন্নিবেশ করুন…
View|দেখুন
Optimize|সর্বোত্তম প্রস্থ
Line numbers|লাইন নম্বর
Inline title|লেখার ভেতরে শিরোনাম
Language|ভাষা
Preferences|পছন্দসমূহ
Preferences…|পছন্দসমূহ…
Recent|সাম্প্রতিক
(empty)|(খালি)
Undo|পূর্বাবস্থা
Redo|পুনরায় করুন
Files|ফাইল
The file is too large (over 5 MB)|ফাইলটি অনেক বড় (৫ MB-এর বেশি)
The file is not a text file|ফাইলটি টেক্সট ফাইল নয়
Ctrl+scroll|Ctrl+স্ক্রল
Paste selection|নির্বাচন পেস্ট করুন
Editor font|সম্পাদকের ফন্ট
Font size|ফন্টের আকার
Text weight|লেখার পুরুত্ব
Bold weight|বোল্ডের পুরুত্ব
Code weight|কোডের পুরুত্ব
Reset|রিসেট
Exit|প্রস্থান
Paste text or image|টেক্সট বা ছবি পেস্ট করুন
Middle click|মাঝের বোতাম
New list item|নতুন তালিকা আইটেম
Indent list item|তালিকা আইটেম ইনডেন্ট করুন
Open link or footnote|লিংক বা পাদটীকা খুলুন
Create new|নতুন তৈরি করুন
File name:|ফাইলের নাম:
Delete…|মুছুন…
Delete|মুছুন
Delete file|ফাইল মুছুন
Move to trash|ট্র্যাশে সরান
Delete permanently|স্থায়ীভাবে মুছুন
Restore default font settings|ডিফল্ট ফন্ট সেটিংস পুনরুদ্ধার করুন
Pasted images|পেস্ট করা ছবি
Embed in the document (JPEG)|নথিতে এমবেড করুন (JPEG)
Otherwise saved as WebP files next to the document|অন্যথায় নথির পাশে WebP ফাইল হিসেবে সংরক্ষণ করুন
Text gamma|টেক্সট গামা
Lower values make text bolder and brighter|কম মান লেখাকে আরও মোটা ও উজ্জ্বল করে
Line height|লাইনের উচ্চতা
Filter|ফিল্টার
characters|অক্ষর
words|শব্দ
links|লিংক
About|সম্পর্কে
Untitled|নামহীন
Find|খুঁজুন
Replace|প্রতিস্থাপন
Next|পরবর্তী
Replace all|সব প্রতিস্থাপন করুন
No matches|কোনো মিল নেই
Unsaved changes|অসংরক্ষিত পরিবর্তন
Save changes to|পরিবর্তন সংরক্ষণ করবেন কি
Don't save|সংরক্ষণ করবেন না
Cancel|বাতিল
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O দিয়ে ফাইল খুলুন বা Ctrl+N দিয়ে নতুন ট্যাবে লিখতে শুরু করুন।
New|নতুন
Keyboard shortcuts|কিবোর্ড শর্টকাট
Bold|বোল্ড
Italic|ইটালিক
Strikethrough|কাটা দাগ
Inline code|ইনলাইন কোড
Highlight|হাইলাইট
Link|লিংক
Heading 1–6|শিরোনাম 1–6
Normal text|সাধারণ লেখা
Bullet list|বুলেট তালিকা
Task (checkbox)|কাজ (চেকবক্স)
Quote|উদ্ধৃতি
License|লাইসেন্স
Code written with the help of|কোড লিখতে সহায়তা করেছে
Save the file first to paste images.|ছবি পেস্ট করার আগে ফাইলটি সংরক্ষণ করুন।
Error|ত্রুটি
"#;

const AR: &str = r#"
File|ملف
Open…|فتح…
Save|حفظ
Save as…|حفظ باسم…
Save all|حفظ الكل
Close|إغلاق
Close all|إغلاق الكل
Edit|تحرير
Copy|نسخ
Paste|لصق
Find/Replace|بحث/استبدال
Insert image…|إدراج صورة…
View|عرض
Optimize|تحسين
Line numbers|أرقام الأسطر
Inline title|العنوان في النص
Language|اللغة
Preferences|التفضيلات
Preferences…|التفضيلات…
Recent|الملفات الأخيرة
(empty)|(فارغ)
Undo|تراجع
Redo|إعادة
Files|الملفات
The file is too large (over 5 MB)|الملف كبير جدًا (أكثر من 5 ميغابايت)
The file is not a text file|الملف ليس ملفًا نصيًا
Ctrl+scroll|Ctrl+عجلة الفأرة
Paste selection|لصق التحديد
Editor font|خط المحرر
Font size|حجم الخط
Text weight|سماكة النص
Bold weight|سماكة الخط العريض
Code weight|سماكة الشيفرة
Reset|إعادة التعيين
Exit|خروج
Paste text or image|لصق نص أو صورة
Middle click|النقر الأوسط
New list item|عنصر قائمة جديد
Indent list item|إزاحة عنصر القائمة
Open link or footnote|فتح الرابط أو الحاشية
Create new|إنشاء جديد
File name:|اسم الملف:
Delete…|حذف…
Delete|حذف
Delete file|حذف الملف
Move to trash|نقل إلى سلة المهملات
Delete permanently|حذف نهائي
Restore default font settings|استعادة إعدادات الخط الافتراضية
Pasted images|الصور الملصقة
Embed in the document (JPEG)|تضمين في المستند (JPEG)
Otherwise saved as WebP files next to the document|وإلا فتُحفظ كملفات WebP بجانب المستند
Text gamma|جاما النص
Lower values make text bolder and brighter|القيم الأقل تجعل النص أسمك وأسطع
Line height|ارتفاع السطر
Filter|تصفية
characters|أحرف
words|كلمات
links|روابط
About|حول
Untitled|بدون عنوان
Find|بحث
Replace|استبدال
Next|التالي
Replace all|استبدال الكل
No matches|لا توجد نتائج
Unsaved changes|تغييرات غير محفوظة
Save changes to|حفظ التغييرات في
Don't save|عدم الحفظ
Cancel|إلغاء
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|افتح ملفًا بـ Ctrl+O أو ابدأ الكتابة في تبويب جديد بـ Ctrl+N.
New|جديد
Keyboard shortcuts|اختصارات لوحة المفاتيح
Bold|عريض
Italic|مائل
Strikethrough|يتوسطه خط
Inline code|شيفرة مضمنة
Highlight|تمييز
Link|رابط
Heading 1–6|العنوان 1–6
Normal text|نص عادي
Bullet list|قائمة نقطية
Task (checkbox)|مهمة (مربع اختيار)
Quote|اقتباس
License|الترخيص
Code written with the help of|الشيفرة مكتوبة بمساعدة
Save the file first to paste images.|احفظ الملف أولًا لتتمكن من لصق الصور.
Error|خطأ
"#;

const UR: &str = r#"
File|فائل
Open…|کھولیں…
Save|محفوظ کریں
Save as…|بطور محفوظ کریں…
Save all|سب محفوظ کریں
Close|بند کریں
Close all|سب بند کریں
Edit|ترمیم
Copy|نقل
Paste|چسپاں کریں
Find/Replace|تلاش/تبدیل
Insert image…|تصویر داخل کریں…
View|منظر
Optimize|بہتر بنائیں
Line numbers|لائن نمبر
Inline title|متن میں عنوان
Language|زبان
Preferences|ترجیحات
Preferences…|ترجیحات…
Recent|حالیہ
(empty)|(خالی)
Undo|واپس
Redo|دوبارہ
Files|فائلیں
The file is too large (over 5 MB)|فائل بہت بڑی ہے (5 MB سے زیادہ)
The file is not a text file|یہ متنی فائل نہیں ہے
Ctrl+scroll|Ctrl+اسکرول
Paste selection|منتخب متن چسپاں کریں
Editor font|ایڈیٹر کا فونٹ
Font size|فونٹ کا سائز
Text weight|متن کی موٹائی
Bold weight|بولڈ کی موٹائی
Code weight|کوڈ کی موٹائی
Reset|ری سیٹ
Exit|باہر نکلیں
Paste text or image|متن یا تصویر چسپاں کریں
Middle click|درمیانی بٹن
New list item|فہرست کا نیا آئٹم
Indent list item|فہرست کا آئٹم اندر کریں
Open link or footnote|لنک یا فٹ نوٹ کھولیں
Create new|نیا بنائیں
File name:|فائل کا نام:
Delete…|حذف کریں…
Delete|حذف کریں
Delete file|فائل حذف کریں
Move to trash|ردی کی ٹوکری میں ڈالیں
Delete permanently|مستقل طور پر حذف کریں
Restore default font settings|ڈیفالٹ فونٹ ترتیبات بحال کریں
Pasted images|چسپاں کی گئی تصاویر
Embed in the document (JPEG)|دستاویز میں شامل کریں (JPEG)
Otherwise saved as WebP files next to the document|ورنہ دستاویز کے ساتھ WebP فائل کے طور پر محفوظ کریں
Text gamma|متن کی گاما
Lower values make text bolder and brighter|کم قدریں متن کو زیادہ گہرا اور روشن بناتی ہیں
Line height|لائن کی اونچائی
Filter|فلٹر
characters|حروف
words|الفاظ
links|لنکس
About|کے بارے میں
Untitled|بے عنوان
Find|تلاش
Replace|تبدیل
Next|اگلا
Replace all|سب تبدیل کریں
No matches|کوئی مماثلت نہیں
Unsaved changes|غیر محفوظ تبدیلیاں
Save changes to|ان میں تبدیلیاں محفوظ کریں
Don't save|محفوظ نہ کریں
Cancel|منسوخ
Open a file with Ctrl+O or start typing in a new tab (Ctrl+N).|Ctrl+O سے فائل کھولیں یا Ctrl+N سے نئے ٹیب میں لکھنا شروع کریں۔
New|نیا
Keyboard shortcuts|کی بورڈ شارٹ کٹس
Bold|بولڈ
Italic|ترچھا
Strikethrough|کاٹا ہوا
Inline code|متن میں کوڈ
Highlight|نمایاں
Link|لنک
Heading 1–6|سرخی 1–6
Normal text|عام متن
Bullet list|نقطوں والی فہرست
Task (checkbox)|کام (چیک باکس)
Quote|اقتباس
License|لائسنس
Code written with the help of|کوڈ کی تحریر میں مدد
Save the file first to paste images.|تصاویر چسپاں کرنے سے پہلے فائل محفوظ کریں۔
Error|خرابی
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back_to_english() {
        assert_eq!(tr(Lang::German, "Save"), "Speichern");
        assert_eq!(tr(Lang::Polish, "Save"), "Zapisz");
        assert_eq!(tr(Lang::English, "Save"), "Save");
        // Not translated yet: shown in English.
        assert_eq!(tr(Lang::Kashubian, "Bold"), "Bold");
        assert_eq!(tr(Lang::Chinese, "No such key"), "No such key");
    }

    #[test]
    fn every_translation_key_is_a_known_string() {
        let keys: Vec<&str> = source(Lang::Polish).lines().filter_map(|l| l.split_once('|')).map(|(k, _)| k).collect();
        for lang in Lang::ALL {
            for line in source(lang).lines().filter(|l| !l.trim().is_empty()) {
                let (key, value) = line.split_once('|').unwrap_or_else(|| panic!("bad line in {lang:?}: {line}"));
                assert!(keys.contains(&key.trim_end()), "{lang:?} has an unknown key {key:?}");
                assert!(!value.trim().is_empty(), "{lang:?} has an empty translation for {key:?}");
            }
        }
    }

    #[test]
    fn locale_codes_map_to_languages() {
        assert_eq!(Lang::from_code("pl_PL.UTF-8"), Some(Lang::Polish));
        assert_eq!(Lang::from_code("zh_CN"), Some(Lang::Chinese));
        assert_eq!(Lang::from_code("csb"), Some(Lang::Kashubian));
        assert_eq!(Lang::from_code("xx"), None);
    }

    #[test]
    fn menu_is_alphabetical_by_english_name() {
        let names: Vec<String> = Lang::ALL.iter().map(|l| format!("{l:?}")).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }
}
