//! Shared structural rules used by Markdown input, numbering and layout.

pub const INDENT_SPACES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListMarker {
    Bullet(u8),
    Task(bool),
    Ordered { digits: usize, delimiter: u8 },
}

#[derive(Clone, Copy, Debug)]
pub struct Prefix {
    pub leading: usize,
    pub quotes: usize,
    pub quote_end: usize,
    pub marker_start: usize,
    pub content_start: usize,
    pub marker: Option<ListMarker>,
}

pub fn whitespace_len(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| matches!(**b, b' ' | b'\t')).count()
}

/// The editor renders two spaces or one tab as one nesting step.
pub fn indent_level(bytes: &[u8]) -> usize {
    bytes.iter().map(|b| if *b == b'\t' { INDENT_SPACES } else { 1 }).sum::<usize>() / INDENT_SPACES
}

/// Canonicalize each whitespace run, preserving the position/depth of quote markers.
pub fn prefix_key(prefix_text: &str) -> String {
    let p = prefix(prefix_text);
    let mut key = " ".repeat(indent_level(&prefix_text.as_bytes()[..p.leading]) * INDENT_SPACES);
    key.push_str(&">".repeat(p.quotes));
    key.push_str(&" ".repeat(indent_level(&prefix_text.as_bytes()[p.quote_end..p.marker_start]) * INDENT_SPACES));
    key
}

pub fn prefix(line: &str) -> Prefix {
    let b = line.as_bytes();
    let leading = whitespace_len(b);
    let mut i = leading;
    let mut quotes = 0;
    while b.get(i) == Some(&b'>') {
        quotes += 1;
        i += if b.get(i + 1) == Some(&b' ') { 2 } else { 1 };
    }
    let quote_end = i;
    i += whitespace_len(&b[i..]);
    let marker_start = i;
    let rest = &line[i..];
    let (marker, size) = if matches!(b.get(i), Some(b'-' | b'*' | b'+'))
        && (i + 1 == b.len() || b.get(i + 1) == Some(&b' '))
    {
        let body = &line[(i + 2).min(b.len())..];
        if ["[ ]", "[x]", "[X]"].iter().any(|t| body == *t || body.strip_prefix(t).is_some_and(|r| r.starts_with(' ')))
        {
            (Some(ListMarker::Task(body.as_bytes()[1] != b' ')), 6.min(b.len() - i))
        } else {
            (Some(ListMarker::Bullet(b[i])), 2.min(b.len() - i))
        }
    } else {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 && matches!(b.get(i + digits), Some(b'.' | b')')) && b.get(i + digits + 1) == Some(&b' ') {
            (Some(ListMarker::Ordered { digits, delimiter: b[i + digits] }), digits + 2)
        } else {
            (None, 0)
        }
    };
    Prefix { leading, quotes, quote_end, marker_start, content_start: i + size, marker }
}

#[derive(Clone, Copy, Debug)]
pub struct Fence {
    marker: u8,
    len: usize,
}

impl Fence {
    pub fn opening(line: &str) -> Option<Self> {
        let b = line.trim_start().as_bytes();
        let marker = *b.first()?;
        if !matches!(marker, b'`' | b'~') {
            return None;
        }
        let len = b.iter().take_while(|b| **b == marker).count();
        (len >= 3).then_some(Self { marker, len })
    }

    pub fn info(self, line: &str) -> &str {
        line.trim_start().get(self.len..).unwrap_or("").trim()
    }

    pub fn closes(self, line: &str) -> bool {
        let t = line.trim();
        t.len() >= self.len && t.bytes().all(|b| b == self.marker)
    }
}

/// Only a closed YAML header at the start of the document is front matter.
pub fn front_matter_end(text: &str) -> Option<usize> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    lines.enumerate().find_map(|(i, line)| matches!(line.trim_end(), "---" | "...").then_some(i + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_share_visual_indentation_and_marker_boundaries() {
        assert_eq!(prefix_key("  "), prefix_key("   "));
        assert_eq!(prefix_key("  "), prefix_key("\t"));
        assert_eq!(prefix_key("> "), prefix_key(">  "));
        assert_eq!(prefix_key(">   "), prefix_key("> \t"));
        assert_ne!(prefix_key("  > "), prefix_key(">   "));
        let line = "\t> >   - [X] żółw";
        let p = prefix(line);
        assert_eq!(p.quotes, 2);
        assert_eq!(p.marker, Some(ListMarker::Task(true)));
        assert_eq!(&line[p.content_start..], "żółw");
        assert_eq!(prefix("123) item").marker, Some(ListMarker::Ordered { digits: 3, delimiter: b')' }));
        assert!(prefix("123.item").marker.is_none());
    }

    #[test]
    fn code_fences_require_matching_character_and_at_least_opening_length() {
        for opening in ["````rust", "~~~~~"] {
            let fence = Fence::opening(opening).unwrap();
            assert!(!fence.closes("```"));
            assert!(!fence.closes("~~~"));
            assert!(!fence.closes(&format!("{} extra", &opening[..4])));
            assert!(fence.closes(&opening[..4].chars().next().unwrap().to_string().repeat(6)));
        }
    }

    #[test]
    fn front_matter_requires_a_closing_delimiter() {
        assert_eq!(front_matter_end("---\nkey: value\n...\ntext"), Some(2));
        assert_eq!(front_matter_end("---\nkey: value\n---\ntext"), Some(2));
        assert_eq!(front_matter_end("---\n1. item\n9. item"), None);
    }
}
