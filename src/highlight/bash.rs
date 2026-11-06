//! Lightweight Bash token coloring; shares the parent module's palette and layout.

use std::collections::VecDeque;

use super::{Class, lines, paint};

struct HereDoc {
    delimiter: Vec<u8>,
    strip_tabs: bool,
    expand: bool,
}

fn name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// A parameter reference, not a command/arithmetic substitution (`$(...)`).
fn parameter_end(b: &[u8], start: usize, end: usize) -> usize {
    let next = b.get(start + 1).copied();
    match next {
        Some(b'{') => {
            let mut depth = 1;
            let mut i = start + 2;
            while i < end {
                match b[i] {
                    b'\\' => {
                        i = (i + 2).min(end);
                        continue;
                    }
                    b'{' => depth += 1,
                    b'}' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            end
        }
        Some(b'0'..=b'9' | b'@' | b'*' | b'#' | b'?' | b'-' | b'$' | b'!') => (start + 2).min(end),
        Some(c) if c.is_ascii_alphabetic() || c == b'_' => {
            let mut i = start + 2;
            while i < end && name_byte(b[i]) {
                i += 1;
            }
            i
        }
        _ => start + 1,
    }
}

fn variables(b: &[u8], start: usize, end: usize, cls: &mut [Class]) {
    let mut i = start;
    while i < end {
        if b[i] == b'\\' {
            i = (i + 2).min(end);
        } else if b[i] == b'$' {
            let stop = parameter_end(b, i, end);
            if stop > i + 1 {
                paint(cls, i, stop, Class::Number);
            }
            i = stop;
        } else {
            i += 1;
        }
    }
}

fn operator(c: u8) -> bool {
    b";|&(){}[]<>!=".contains(&c)
}

/// Read and quote-remove a here-document delimiter; `<<<` is a here-string instead.
fn heredoc(b: &[u8], start: usize, end: usize) -> Option<(HereDoc, usize)> {
    if !b[start..end].starts_with(b"<<") || b.get(start + 2) == Some(&b'<') {
        return None;
    }
    let mut i = start + 2;
    let strip_tabs = b.get(i) == Some(&b'-');
    i += usize::from(strip_tabs);
    while i < end && matches!(b[i], b' ' | b'\t') {
        i += 1;
    }
    let mut delimiter = Vec::new();
    let mut quote = None;
    let mut expand = true;
    let mut seen = false;
    while i < end {
        let c = b[i];
        if quote.is_none() && (c.is_ascii_whitespace() || operator(c)) {
            break;
        }
        seen = true;
        if c == b'\\' && quote != Some(b'\'') && i + 1 < end {
            expand = false;
            i += 1;
            delimiter.push(b[i]);
        } else if quote == Some(c) {
            quote = None;
        } else if quote.is_none() && matches!(c, b'\'' | b'"') {
            quote = Some(c);
            expand = false;
        } else {
            delimiter.push(c);
        }
        i += 1;
    }
    (seen && quote.is_none()).then_some((HereDoc { delimiter, strip_tabs, expand }, i))
}

pub(super) fn classify(text: &str, cls: &mut [Class]) {
    let b = text.as_bytes();
    let mut quote = None;
    let mut ansi_quote = false;
    let mut arithmetic = 0usize;
    let mut docs: VecDeque<HereDoc> = VecDeque::new();
    for (start, end) in lines(text) {
        if let Some(doc) = docs.front() {
            let body = &b[start..end];
            let body = if doc.strip_tabs { &body[body.iter().take_while(|c| **c == b'\t').count()..] } else { body };
            if body == doc.delimiter {
                paint(cls, start, end, Class::Punct);
                docs.pop_front();
            } else {
                paint(cls, start, end, Class::Str);
                if doc.expand {
                    variables(b, start, end, cls);
                }
            }
            continue;
        }
        let mut i = start;
        let mut word_start = true;
        while i < end {
            let c = b[i];
            if c == b'\\'
                && (quote.is_none()
                    || ansi_quote
                    || (quote == Some(b'"') && b.get(i + 1).is_some_and(|c| b"$`\"\\".contains(c))))
            {
                let stop = (i + 2).min(end);
                if quote.is_some() {
                    paint(cls, i, stop, Class::Str);
                }
                i = stop;
                word_start = false;
                continue;
            }
            if let Some(q) = quote {
                if c == b'$' && q == b'"' {
                    let stop = parameter_end(b, i, end);
                    paint(cls, i, stop, if stop > i + 1 { Class::Number } else { Class::Str });
                    i = stop;
                    continue;
                }
                cls[i] = Class::Str;
                if c == q {
                    quote = None;
                    ansi_quote = false;
                }
                i += 1;
                continue;
            }
            match c {
                b'#' if word_start => {
                    paint(cls, i, end, Class::Comment);
                    break;
                }
                b'\'' | b'"' => {
                    quote = Some(c);
                    ansi_quote = c == b'\'' && i > start && b[i - 1] == b'$';
                    cls[i] = Class::Str;
                    word_start = false;
                    i += 1;
                }
                b'$' => {
                    let stop = parameter_end(b, i, end);
                    paint(cls, i, stop, if stop > i + 1 { Class::Number } else { Class::Punct });
                    i = stop;
                    word_start = false;
                }
                b'(' if b.get(i + 1) == Some(&b'(') => {
                    arithmetic += 1;
                    paint(cls, i, i + 2, Class::Punct);
                    i += 2;
                    word_start = true;
                }
                b')' if arithmetic > 0 && b.get(i + 1) == Some(&b')') => {
                    arithmetic -= 1;
                    paint(cls, i, i + 2, Class::Punct);
                    i += 2;
                    word_start = true;
                }
                b'<' if b[i..end].starts_with(b"<<<") => {
                    paint(cls, i, i + 3, Class::Punct);
                    i += 3;
                    word_start = true;
                }
                b'<' if arithmetic == 0 && b.get(i + 1) == Some(&b'<') && b.get(i + 2) != Some(&b'<') => {
                    if let Some((doc, stop)) = heredoc(b, i, end) {
                        paint(cls, i, stop, Class::Punct);
                        docs.push_back(doc);
                        i = stop;
                    } else {
                        cls[i] = Class::Punct;
                        i += 1;
                    }
                    word_start = true;
                }
                c if c.is_ascii_whitespace() => {
                    word_start = true;
                    i += 1;
                }
                c if operator(c) => {
                    cls[i] = Class::Punct;
                    word_start = c != b'=';
                    i += 1;
                }
                _ => {
                    let mut stop = i + 1;
                    while stop < end
                        && !b[stop].is_ascii_whitespace()
                        && !operator(b[stop])
                        && !b"'\"\\$".contains(&b[stop])
                    {
                        stop += 1;
                    }
                    let token = &b[i..stop];
                    let assignment = word_start
                        && b.get(stop) == Some(&b'=')
                        && (token[0].is_ascii_alphabetic() || token[0] == b'_')
                        && token.iter().all(|c| name_byte(*c));
                    let keyword = matches!(
                        token,
                        b"if"
                            | b"then"
                            | b"else"
                            | b"elif"
                            | b"fi"
                            | b"for"
                            | b"while"
                            | b"until"
                            | b"do"
                            | b"done"
                            | b"case"
                            | b"esac"
                            | b"in"
                            | b"select"
                            | b"function"
                            | b"time"
                            | b"coproc"
                            | b"export"
                            | b"local"
                            | b"readonly"
                            | b"declare"
                            | b"typeset"
                            | b"unset"
                            | b"source"
                            | b"return"
                            | b"break"
                            | b"continue"
                            | b"exit"
                            | b"echo"
                            | b"printf"
                            | b"read"
                            | b"cd"
                            | b"pwd"
                            | b"set"
                            | b"shopt"
                            | b"eval"
                            | b"exec"
                            | b"trap"
                            | b"test"
                            | b"true"
                            | b"false"
                    );
                    let class = if assignment {
                        Class::Key
                    } else if keyword {
                        Class::Keyword
                    } else if token.iter().all(u8::is_ascii_digit) {
                        Class::Number
                    } else {
                        Class::Text
                    };
                    paint(cls, i, stop, class);
                    i = stop;
                    word_start = false;
                }
            }
        }
    }
}
