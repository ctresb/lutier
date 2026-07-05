// Lexer for the .synth patch DSL.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Id(String),
    Num(f64, String), // value, unit suffix ("" if none)
    Str(String),
    Sym(String), // { } ( ) : , + - * / -> ..
}

/// (line, col), both 1-based, of each token
pub type Span = (u32, u32);

pub fn lex(src: &str) -> Result<Vec<Tok>, String> {
    lex_spanned(src).map(|(t, _)| t)
}

pub fn lex_spanned(src: &str) -> Result<(Vec<Tok>, Vec<Span>), String> {
    let b: Vec<char> = src.chars().collect();
    // precompute (line, col) per char index
    let mut linecol = Vec::with_capacity(b.len() + 1);
    let (mut line, mut col) = (1u32, 1u32);
    for &c in &b {
        linecol.push((line, col));
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    linecol.push((line, col));
    let mut spans = Vec::new();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i];
        let tok_start = i;
        if c == '#' {
            while i < b.len() && b[i] != '\n' {
                i += 1;
            }
        } else if c.is_whitespace() {
            i += 1;
        } else if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < b.len() && b[i] != '"' {
                s.push(b[i]);
                i += 1;
            }
            i += 1;
            out.push(Tok::Str(s));
        } else if c.is_ascii_digit() || (c == '.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let start = i;
            let mut seen_dot = false;
            while i < b.len() {
                let ch = b[i];
                if ch.is_ascii_digit() || ch == '_' {
                    i += 1;
                } else if ch == '.' && !seen_dot && !(i + 1 < b.len() && b[i + 1] == '.') {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E')
                    && i + 1 < b.len()
                    && (b[i + 1].is_ascii_digit() || b[i + 1] == '-')
                    // don't eat unit letters: only exponent if digits follow
                    && {
                        let mut j = i + 1;
                        if b[j] == '-' { j += 1; }
                        j < b.len() && b[j].is_ascii_digit()
                    }
                {
                    i += 1;
                    if b[i] == '-' {
                        i += 1;
                    }
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    break;
                } else {
                    break;
                }
            }
            let numstr: String = b[start..i].iter().filter(|&&c| c != '_').collect();
            let val: f64 = numstr.parse().map_err(|_| {
                let (l, c) = linecol[tok_start];
                format!("{}:{}: bad number: {}", l, c, numstr)
            })?;
            // unit suffix: letters or %
            let mut unit = String::new();
            while i < b.len() && (b[i].is_ascii_alphabetic() || b[i] == '%') {
                unit.push(b[i]);
                i += 1;
                if unit == "%" {
                    break;
                }
            }
            out.push(Tok::Num(val, unit));
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == '_') {
                i += 1;
            }
            out.push(Tok::Id(b[start..i].iter().collect()));
        } else if c == '-' && i + 1 < b.len() && b[i + 1] == '>' {
            out.push(Tok::Sym("->".into()));
            i += 2;
        } else if c == '.' && i + 1 < b.len() && b[i + 1] == '.' {
            out.push(Tok::Sym("..".into()));
            i += 2;
        } else if "{}():,+-*/=.[]".contains(c) {
            out.push(Tok::Sym(c.to_string()));
            i += 1;
        } else {
            let (l, cl) = linecol[tok_start];
            return Err(format!("{}:{}: unexpected char: {:?}", l, cl, c));
        }
        while spans.len() < out.len() {
            spans.push(linecol[tok_start]);
        }
    }
    Ok((out, spans))
}
