//! Converts clipboard HTML (browsers, office suites) to Markdown without external dependencies.

enum Item {
    Bullet,
    Ordered(usize),
}

#[derive(Default)]
struct Converter {
    out: String,
    lists: Vec<Item>,
    links: Vec<String>,
    quotes: Vec<usize>,
    pre: bool,
    /// Tag whose content is dropped (script, style, head...) until its closing tag.
    skip: Option<String>,
}

impl Converter {
    fn line(&mut self) {
        if !self.out.is_empty() && !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn block(&mut self) {
        self.line();
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            self.out.push('\n');
        }
    }

    fn text(&mut self, raw: &str) {
        let text = decode(raw);
        if self.pre {
            self.out.push_str(&text);
            return;
        }
        let mut pending = false;
        for ch in text.chars() {
            if ch.is_whitespace() && ch != '\u{a0}' {
                pending = true;
                continue;
            }
            if pending && !self.out.is_empty() && !self.out.ends_with([' ', '\n', '\t']) {
                self.out.push(' ');
            }
            pending = false;
            self.out.push(ch);
        }
        if pending && !self.out.is_empty() && !self.out.ends_with([' ', '\n', '\t']) {
            self.out.push(' ');
        }
    }

    fn open(&mut self, name: &str, attrs: &str) {
        match name {
            "script" | "style" | "head" | "title" | "template" => self.skip = Some(name.to_owned()),
            "p" => self.block(),
            "div" | "section" | "article" | "header" | "footer" | "tr" | "table" => self.line(),
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.block();
                let level = name[1..].parse().unwrap_or(1);
                self.out.push_str(&"#".repeat(level));
                self.out.push(' ');
            }
            "br" => self.out.push_str("  \n"),
            "hr" => {
                self.block();
                self.out.push_str("---\n\n");
            }
            "b" | "strong" => self.out.push_str("**"),
            "i" | "em" => self.out.push('*'),
            "s" | "del" | "strike" => self.out.push_str("~~"),
            "code" if !self.pre => self.out.push('`'),
            "pre" => {
                self.block();
                self.out.push_str("```\n");
                self.pre = true;
            }
            "a" => {
                let href = attr(attrs, "href").unwrap_or_default();
                if !href.is_empty() {
                    self.out.push('[');
                }
                self.links.push(href);
            }
            "img" => {
                if let Some(src) = attr(attrs, "src").filter(|s| !s.is_empty()) {
                    let alt = attr(attrs, "alt").unwrap_or_default();
                    self.out.push_str(&format!("![{alt}]({src})"));
                }
            }
            "ul" | "ol" => {
                if self.lists.is_empty() {
                    self.block();
                } else {
                    self.line();
                }
                self.lists.push(if name == "ol" { Item::Ordered(0) } else { Item::Bullet });
            }
            "li" => {
                self.line();
                let depth = self.lists.len().saturating_sub(1);
                self.out.push_str(&"\t".repeat(depth));
                match self.lists.last_mut() {
                    Some(Item::Ordered(n)) => {
                        *n += 1;
                        self.out.push_str(&format!("{n}. "));
                    }
                    _ => self.out.push_str("- "),
                }
            }
            "blockquote" => {
                self.block();
                self.quotes.push(self.out.len());
            }
            _ => {}
        }
    }

    fn close(&mut self, name: &str) {
        match name {
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.block(),
            "div" | "section" | "article" | "header" | "footer" | "tr" | "table" => self.line(),
            "td" | "th" => self.out.push(' '),
            "b" | "strong" => self.out.push_str("**"),
            "i" | "em" => self.out.push('*'),
            "s" | "del" | "strike" => self.out.push_str("~~"),
            "code" if !self.pre => self.out.push('`'),
            "pre" => {
                self.line();
                self.out.push_str("```");
                self.pre = false;
                self.block();
            }
            "a" => {
                if let Some(href) = self.links.pop().filter(|h| !h.is_empty()) {
                    self.out.push_str(&format!("]({href})"));
                }
            }
            "ul" | "ol" => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.block();
                } else {
                    self.line();
                }
            }
            "blockquote" => {
                if let Some(start) = self.quotes.pop() {
                    let body = self.out.split_off(start);
                    let quoted: Vec<String> = body
                        .trim()
                        .lines()
                        .map(|l| if l.is_empty() { ">".to_owned() } else { format!("> {l}") })
                        .collect();
                    self.out.push_str(&quoted.join("\n"));
                    self.block();
                }
            }
            _ => {}
        }
    }
}

/// Value of attribute `name` in the raw attribute text of a tag.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let boundary = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = attrs[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=').filter(|_| boundary) else { continue };
        let rest = rest.trim_start();
        let value = match rest.chars().next()? {
            q @ ('"' | '\'') => rest[1..].split(q).next()?,
            _ => rest.split(char::is_whitespace).next()?,
        };
        return Some(decode(value));
    }
    None
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let ch = match &rest[1..end] {
                "amp" => '&',
                "lt" => '<',
                "gt" => '>',
                "quot" => '"',
                "apos" => '\'',
                "nbsp" => '\u{a0}',
                num if num.starts_with('#') => {
                    let num = &num[1..];
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)?
                }
                _ => return None,
            };
            Some((ch, end + 1))
        });
        match decoded {
            Some((ch, len)) => {
                out.push(ch);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// End of the tag starting at `html[0] == '<'`, honouring quoted attribute values.
fn tag_end(html: &str) -> Option<usize> {
    let mut quote = None;
    for (i, ch) in html.char_indices() {
        match (quote, ch) {
            (None, '"' | '\'') => quote = Some(ch),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

pub fn to_markdown(html: &str) -> String {
    let mut conv = Converter::default();
    // Windows CF_HTML prefixes the markup with a plain-text header.
    let mut rest = if html.starts_with("Version:") { &html[html.find('<').unwrap_or(html.len())..] } else { html };
    while !rest.is_empty() {
        if let Some(tail) = rest.strip_prefix("<!--") {
            rest = tail.find("-->").map_or("", |i| &tail[i + 3..]);
        } else if rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/') {
            let Some(end) = tag_end(rest) else { break };
            let inner = rest[1..end].trim_end_matches('/');
            rest = &rest[end + 1..];
            let (closing, inner) = inner.strip_prefix('/').map_or((false, inner), |i| (true, i));
            let (name, attrs) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
            let name = name.to_ascii_lowercase();
            match (&conv.skip, closing) {
                (Some(skip), true) if *skip == name => conv.skip = None,
                (Some(_), _) => {}
                (None, true) => conv.close(&name),
                (None, false) => conv.open(&name, attrs),
            }
        } else {
            let end = rest[1..].find('<').map_or(rest.len(), |i| i + 1);
            if conv.skip.is_none() {
                conv.text(&rest[..end]);
            }
            rest = &rest[end..];
        }
    }
    let mut out = conv.out.trim().to_owned();
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::to_markdown;

    #[test]
    fn converts_inline_formatting_links_and_entities() {
        let html = "<html><body><!--StartFragment--><p>A <b>bold</b> &amp; <em>it</em> <a href=\"https://x.org/?a=1&amp;b=2\">link</a> <code>c&lt;1&gt;</code></p><!--EndFragment--></body></html>";
        assert_eq!(to_markdown(html), "A **bold** & *it* [link](https://x.org/?a=1&b=2) `c<1>`");
    }

    #[test]
    fn converts_blocks_lists_and_quotes() {
        let html = "<h2>Title</h2><p>One<br>two</p><ul><li>a<ul><li>b</li></ul></li><li>c</li></ul>\
                    <ol><li>x</li><li>y</li></ol><blockquote><p>q</p></blockquote><pre>let a = 1;\nlet b = 2;</pre>";
        assert_eq!(
            to_markdown(html),
            "## Title\n\nOne  \ntwo\n\n- a\n\t- b\n- c\n\n1. x\n2. y\n\n> q\n\n```\nlet a = 1;\nlet b = 2;\n```"
        );
    }

    #[test]
    fn drops_scripts_and_keeps_plain_text() {
        assert_eq!(to_markdown("<style>p{}</style><div>a</div><div>b</div><script>x()</script>"), "a\nb");
        assert_eq!(to_markdown("1 < 2 &#x41;"), "1 < 2 A");
    }
}
