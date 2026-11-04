//! A small LaTeX-math to Unicode converter, good enough to display typical formulas
//! (`\frac`, `\sqrt`, `\text`, `\boxed`, Greek letters, operators, simple sub/superscripts).

/// Converts LaTeX math to plain Unicode text. Returns the text and whether the formula
/// was wrapped in `\boxed{…}` at the top level.
pub fn to_unicode(src: &str) -> (String, bool) {
    let mut p = Parser { c: src.chars().collect(), i: 0, boxed: false };
    let s = p.seq(None, 0);
    (collapse_spaces(&s), p.boxed)
}

struct Parser {
    c: Vec<char>,
    i: usize,
    boxed: bool,
}

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }

    /// Atoms until `end` (consumed) or the end of input.
    fn seq(&mut self, end: Option<char>, depth: usize) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek() {
            if Some(c) == end {
                self.i += 1;
                break;
            }
            out += &self.atom(depth);
        }
        out
    }

    /// A `{…}` group or a single atom (an argument of a command).
    fn group(&mut self, depth: usize) -> String {
        while self.peek() == Some(' ') {
            self.i += 1;
        }
        if self.peek() == Some('{') {
            self.i += 1;
            self.seq(Some('}'), depth + 1)
        } else {
            self.atom(depth + 1)
        }
    }

    fn atom(&mut self, depth: usize) -> String {
        let Some(c) = self.peek() else { return String::new() };
        self.i += 1;
        match c {
            '\\' => self.command(depth),
            '{' => self.seq(Some('}'), depth + 1),
            '^' => script(&self.group(depth), SUP, '^'),
            '_' => script(&self.group(depth), SUB, '_'),
            '~' | '&' => " ".into(),
            c => c.to_string(),
        }
    }

    fn command(&mut self, depth: usize) -> String {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        if self.i == start {
            let Some(c) = self.peek() else { return String::new() };
            self.i += 1;
            return match c {
                '\\' => "\n".into(),
                ',' | ';' | ':' | ' ' => " ".into(),
                '!' => String::new(),
                c => c.to_string(),
            };
        }
        let name: String = self.c[start..self.i].iter().collect();
        match name.as_str() {
            "text" | "textrm" | "textbf" | "textit" | "textsf" | "texttt" | "textnormal" | "mathrm" | "mathbf"
            | "mathit" | "mathsf" | "mathtt" | "mathcal" | "mathfrak" | "boldsymbol" | "operatorname" | "mbox"
            | "hbox" | "overline" | "underline" | "hat" | "widehat" | "bar" | "vec" | "tilde" | "widetilde" | "dot"
            | "ddot" => self.group(depth),
            "mathbb" => self.group(depth).chars().map(double_struck).collect(),
            "boxed" | "fbox" => {
                let g = self.group(depth);
                if depth == 0 {
                    self.boxed = true;
                    g
                } else {
                    format!("[{g}]")
                }
            }
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let (a, b) = (self.group(depth), self.group(depth));
                format!("{}/{}", paren(&a), paren(&b))
            }
            "binom" => {
                let (a, b) = (self.group(depth), self.group(depth));
                format!("C({a}, {b})")
            }
            "sqrt" => {
                let mut root = String::new();
                if self.peek() == Some('[') {
                    self.i += 1;
                    root = script(&self.seq(Some(']'), depth + 1), SUP, '^');
                }
                format!("{root}√{}", paren(&self.group(depth)))
            }
            "left" | "right" | "bigl" | "bigr" | "Bigl" | "Bigr" | "big" | "Big" | "bigg" | "Bigg" => {
                if self.peek() == Some('.') {
                    self.i += 1;
                }
                String::new()
            }
            "begin" | "end" => {
                self.group(depth);
                String::new()
            }
            "displaystyle" | "textstyle" | "limits" | "nolimits" => String::new(),
            "quad" => "  ".into(),
            "qquad" => "    ".into(),
            "log" | "ln" | "lg" | "exp" | "sin" | "cos" | "tan" | "cot" | "sec" | "csc" | "arcsin" | "arccos"
            | "arctan" | "sinh" | "cosh" | "tanh" | "lim" | "max" | "min" | "sup" | "inf" | "det" | "arg" | "deg"
            | "gcd" | "mod" | "dim" | "ker" | "Pr" => format!("{name} "),
            _ => symbol(&name).map_or(name, str::to_owned),
        }
    }
}

fn symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" | "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" | "varrho" => "ρ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" | "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        "times" => "×",
        "cdot" => "·",
        "div" => "÷",
        "pm" => "±",
        "mp" => "∓",
        "le" | "leq" => "≤",
        "ge" | "geq" => "≥",
        "ll" => "≪",
        "gg" => "≫",
        "ne" | "neq" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "simeq" => "≃",
        "cong" => "≅",
        "propto" => "∝",
        "infty" => "∞",
        "to" | "rightarrow" => "→",
        "leftarrow" | "gets" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "Leftrightarrow" | "iff" => "⇔",
        "leftrightarrow" => "↔",
        "implies" => "⟹",
        "impliedby" => "⟸",
        "mapsto" => "↦",
        "uparrow" => "↑",
        "downarrow" => "↓",
        "sum" => "∑",
        "prod" => "∏",
        "coprod" => "∐",
        "int" => "∫",
        "iint" => "∬",
        "oint" => "∮",
        "partial" => "∂",
        "nabla" => "∇",
        "forall" => "∀",
        "exists" => "∃",
        "nexists" => "∄",
        "in" => "∈",
        "notin" => "∉",
        "ni" => "∋",
        "subset" => "⊂",
        "subseteq" => "⊆",
        "supset" => "⊃",
        "supseteq" => "⊇",
        "cup" => "∪",
        "cap" => "∩",
        "setminus" => "∖",
        "emptyset" | "varnothing" => "∅",
        "land" | "wedge" => "∧",
        "lor" | "vee" => "∨",
        "neg" | "lnot" => "¬",
        "cdots" => "⋯",
        "ldots" | "dots" => "…",
        "vdots" => "⋮",
        "ddots" => "⋱",
        "circ" => "∘",
        "bullet" => "•",
        "star" => "⋆",
        "ast" => "∗",
        "oplus" => "⊕",
        "otimes" => "⊗",
        "langle" => "⟨",
        "rangle" => "⟩",
        "lfloor" => "⌊",
        "rfloor" => "⌋",
        "lceil" => "⌈",
        "rceil" => "⌉",
        "mid" | "vert" => "|",
        "Vert" | "parallel" => "‖",
        "perp" => "⊥",
        "angle" => "∠",
        "degree" => "°",
        "prime" => "′",
        "hbar" => "ℏ",
        "ell" => "ℓ",
        "Re" => "ℜ",
        "Im" => "ℑ",
        "aleph" => "ℵ",
        "checkmark" => "✓",
        _ => return None,
    })
}

fn double_struck(c: char) -> char {
    match c {
        'R' => 'ℝ',
        'N' => 'ℕ',
        'Z' => 'ℤ',
        'Q' => 'ℚ',
        'C' => 'ℂ',
        'P' => 'ℙ',
        'E' => '𝔼',
        c => c,
    }
}

const SUP: &[(char, char)] = &[
    ('0', '⁰'),
    ('1', '¹'),
    ('2', '²'),
    ('3', '³'),
    ('4', '⁴'),
    ('5', '⁵'),
    ('6', '⁶'),
    ('7', '⁷'),
    ('8', '⁸'),
    ('9', '⁹'),
    ('+', '⁺'),
    ('-', '⁻'),
    ('=', '⁼'),
    ('(', '⁽'),
    (')', '⁾'),
    ('n', 'ⁿ'),
    ('i', 'ⁱ'),
    ('a', 'ᵃ'),
    ('b', 'ᵇ'),
    ('c', 'ᶜ'),
    ('d', 'ᵈ'),
    ('e', 'ᵉ'),
    ('f', 'ᶠ'),
    ('g', 'ᵍ'),
    ('h', 'ʰ'),
    ('j', 'ʲ'),
    ('k', 'ᵏ'),
    ('l', 'ˡ'),
    ('m', 'ᵐ'),
    ('o', 'ᵒ'),
    ('p', 'ᵖ'),
    ('r', 'ʳ'),
    ('s', 'ˢ'),
    ('t', 'ᵗ'),
    ('u', 'ᵘ'),
    ('v', 'ᵛ'),
    ('w', 'ʷ'),
    ('x', 'ˣ'),
    ('y', 'ʸ'),
    ('z', 'ᶻ'),
    ('T', 'ᵀ'),
    ('′', '′'),
];

const SUB: &[(char, char)] = &[
    ('0', '₀'),
    ('1', '₁'),
    ('2', '₂'),
    ('3', '₃'),
    ('4', '₄'),
    ('5', '₅'),
    ('6', '₆'),
    ('7', '₇'),
    ('8', '₈'),
    ('9', '₉'),
    ('+', '₊'),
    ('-', '₋'),
    ('=', '₌'),
    ('(', '₍'),
    (')', '₎'),
    ('a', 'ₐ'),
    ('e', 'ₑ'),
    ('h', 'ₕ'),
    ('i', 'ᵢ'),
    ('j', 'ⱼ'),
    ('k', 'ₖ'),
    ('l', 'ₗ'),
    ('m', 'ₘ'),
    ('n', 'ₙ'),
    ('o', 'ₒ'),
    ('p', 'ₚ'),
    ('r', 'ᵣ'),
    ('s', 'ₛ'),
    ('t', 'ₜ'),
    ('u', 'ᵤ'),
    ('v', 'ᵥ'),
    ('x', 'ₓ'),
];

/// Maps to Unicode super/subscript characters when all of them exist, else `^(…)` / `_(…)`.
fn script(s: &str, table: &[(char, char)], fallback: char) -> String {
    let s = s.trim();
    let mapped: Option<String> =
        s.chars().map(|c| table.iter().find(|(from, _)| *from == c).map(|(_, to)| *to)).collect();
    match mapped {
        Some(m) if !m.is_empty() => m,
        _ if s.chars().count() == 1 => format!("{fallback}{s}"),
        _ => format!("{fallback}({s})"),
    }
}

fn paren(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() > 1 && s.contains([' ', '+', '-', '·', '×', '/']) { format!("({s})") } else { s.to_owned() }
}

fn collapse_spaces(s: &str) -> String {
    let lines: Vec<String> = s.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    lines.into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::to_unicode;

    #[test]
    fn converts_common_formulas() {
        let (s, boxed) = to_unicode(r"\boxed{ \text{Automatyczna kompilacja} + \text{model} }");
        assert_eq!(s, "Automatyczna kompilacja + model");
        assert!(boxed);
        assert_eq!(to_unicode(r"\frac{a+b}{2} \le \sqrt{x^2 + y_1}").0, "(a+b)/2 ≤ √(x² + y₁)");
        assert_eq!(to_unicode(r"\sum_{i=1}^{n} \alpha_i \to \infty").0, "∑ᵢ₌₁ⁿ αᵢ → ∞");
        assert_eq!(to_unicode(r"\mathbb{R}^n").0, "ℝⁿ");
    }
}
