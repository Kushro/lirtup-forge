//! The BGRA ↔ RGBA swizzle around the chain (capture is BGRA, effects expect
//! RGBA; `SWAP_RB_EFFECT` in 1.x).
//!
//! Two ways, as in 1.x `swizzle.py`:
//!
//! - folded into the shaders ([`rewrite`]): the *preprocessed* pass source
//!   is rewritten so every read of `INPUT` (`Load`, `Sample*`, `[]`) returns
//!   swapped channels (`GatherRed` and `GatherBlue` trade places) and every
//!   store `OUTPUT[...] = v;` stores `v.bgra`;
//! - a separate pass ([`swap_rb_source`]) when the effect uses `INPUT` or
//!   `OUTPUT` some other way (FXAA passes `INPUT` to a function).

/// HLSL of the swap-R/B pass (a MagpieFX effect, `internal/SwapRB.hlsl`).
pub fn swap_rb_source() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/internal/SwapRB.hlsl"
    ))
}

/// The function the rewritten source calls.
pub const SWAP_FUNC: &str = "float4 __MP_SwapRB(float4 v) { return v.bgra; }\n";

const SWAP_READS: [&str; 5] = ["Load", "Sample", "SampleLevel", "SampleGrad", "SampleBias"];
const PASSTHROUGH: [&str; 1] = ["GetDimensions"];

fn gather_swap(method: &str) -> Option<&'static str> {
    Some(match method {
        "Gather" | "GatherRed" => "GatherBlue",
        "GatherBlue" => "GatherRed",
        "GatherGreen" => "GatherGreen",
        "GatherAlpha" => "GatherAlpha",
        _ => return None,
    })
}

/// The pass uses `INPUT`/`OUTPUT` in a way the rewriter cannot swap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwapUnsupported(pub String);

impl std::fmt::Display for SwapUnsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SwapUnsupported {}

type SwapResult<T> = std::result::Result<T, SwapUnsupported>;

fn is_word(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Next `INPUT`/`OUTPUT` token at or after `from` that is not part of a
/// longer identifier nor a member (`x.INPUT`).
fn find_token(s: &str, from: usize, names: &[&str]) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let mut i = from;
    while i < b.len() {
        for name in names {
            if b[i..].starts_with(name.as_bytes()) {
                let before_ok = i == 0 || !(is_word(b[i - 1]) || b[i - 1] == b'.');
                let end = i + name.len();
                let after_ok = end >= b.len() || !is_word(b[end]);
                if before_ok && after_ok {
                    return Some((i, end));
                }
            }
        }
        i += 1;
    }
    None
}

/// Index just past the bracket matching `s[i] == open`.
fn matching(s: &str, i: usize, open: u8, close: u8) -> SwapResult<usize> {
    let mut depth = 0i32;
    for (j, c) in s.bytes().enumerate().skip(i) {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Ok(j + 1);
            }
        }
    }
    Err(SwapUnsupported(format!("unbalanced '{}'", open as char)))
}

/// Index of the `;` ending the expression that starts at `i`.
fn statement_end(s: &str, i: usize) -> SwapResult<usize> {
    let mut depth = 0i32;
    for (j, c) in s.bytes().enumerate().skip(i) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b';' if depth == 0 => return Ok(j),
            _ => {}
        }
    }
    Err(SwapUnsupported("assignment without ';'".to_owned()))
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    let b = s.as_bytes();
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn word_at(s: &str, i: usize) -> &str {
    let b = s.as_bytes();
    let mut j = i;
    while j < b.len() && is_word(b[j]) {
        j += 1;
    }
    &s[i..j]
}

/// Rewrite `source` and return `(new_source, input_swapped, output_swapped)`;
/// a side is only reported when the pass touches that texture.
pub fn rewrite(source: &str, swap_input: bool, swap_output: bool) -> SwapResult<(String, bool, bool)> {
    let mut names = Vec::new();
    if swap_input {
        names.push("INPUT");
    }
    if swap_output {
        names.push("OUTPUT");
    }
    if names.is_empty() {
        return Ok((source.to_owned(), false, false));
    }
    let mut used = (false, false);
    let mut out = rewrite_inner(source, &names, &mut used)?;
    if used.0 || used.1 {
        out = format!("{SWAP_FUNC}{out}");
    }
    Ok((out, used.0, used.1))
}

fn rewrite_inner(s: &str, names: &[&str], used: &mut (bool, bool)) -> SwapResult<String> {
    let mut out = String::with_capacity(s.len() + 64);
    let mut pos = 0;
    loop {
        let Some((start, end)) = find_token(s, pos, names) else {
            out.push_str(&s[pos..]);
            return Ok(out);
        };
        let name = &s[start..end];
        out.push_str(&s[pos..start]);
        let i = skip_ws(s, end);

        // Declaration: "... NAME : register(...)"
        if s[i..].starts_with(':') {
            out.push_str(&s[start..i]);
            pos = i;
            continue;
        }

        if name == "INPUT" {
            if s[i..].starts_with('.') {
                let j = skip_ws(s, i + 1);
                let method = word_at(s, j);
                let k = j + method.len();
                if let Some(swapped) = gather_swap(method) {
                    out.push_str("INPUT.");
                    out.push_str(swapped);
                    pos = k;
                } else if SWAP_READS.contains(&method) {
                    let k2 = skip_ws(s, k);
                    if !s[k2..].starts_with('(') {
                        return Err(SwapUnsupported(format!("INPUT.{method} without call")));
                    }
                    let close = matching(s, k2, b'(', b')')?;
                    let inner = rewrite_inner(&s[k2..close], names, used)?;
                    out.push_str(&format!("__MP_SwapRB(INPUT.{method}{inner})"));
                    pos = close;
                } else if PASSTHROUGH.contains(&method) {
                    out.push_str(&s[start..k]);
                    pos = k;
                    continue;
                } else {
                    return Err(SwapUnsupported(format!("INPUT.{method}")));
                }
                used.0 = true;
                continue;
            }
            if s[i..].starts_with('[') {
                let close = matching(s, i, b'[', b']')?;
                let inner = rewrite_inner(&s[i..close], names, used)?;
                out.push_str(&format!("__MP_SwapRB(INPUT{inner})"));
                used.0 = true;
                pos = close;
                continue;
            }
            return Err(SwapUnsupported(
                "INPUT used as a value (e.g. passed to a function)".to_owned(),
            ));
        }

        // OUTPUT
        if s[i..].starts_with('[') {
            let close = matching(s, i, b'[', b']')?;
            let index = rewrite_inner(&s[i..close], names, used)?;
            let k = skip_ws(s, close);
            if !s[k..].starts_with('=') || s[k..].starts_with("==") {
                return Err(SwapUnsupported("OUTPUT read or compound assignment".to_owned()));
            }
            let semi = statement_end(s, k + 1)?;
            let value = rewrite_inner(&s[k + 1..semi], names, used)?;
            out.push_str(&format!("OUTPUT{index} = __MP_SwapRB({})", value.trim()));
            used.1 = true;
            pos = semi;
            continue;
        }
        if s[i..].starts_with('.') {
            let j = skip_ws(s, i + 1);
            let method = word_at(s, j);
            if PASSTHROUGH.contains(&method) {
                out.push_str(&s[start..j + method.len()]);
                pos = j + method.len();
                continue;
            }
        }
        return Err(SwapUnsupported(
            "OUTPUT used as a value (e.g. passed to a function)".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_stores_are_swapped() {
        let src = "Texture2D<float4> INPUT : register(t0);\n\
                   float4 a = INPUT.SampleLevel(sam, pos, 0);\n\
                   float4 b = INPUT[gxy];\n\
                   float4 r = INPUT.GatherRed(sam, pos);\n\
                   OUTPUT[gxy] = a + b;\n\
                   uint w; INPUT.GetDimensions(w, w);\n\
                   float MY_INPUT = 1; s.INPUT = 2;";
        let (out, i, o) = rewrite(src, true, true).unwrap();
        assert!(i && o);
        assert!(out.starts_with(SWAP_FUNC));
        assert!(out.contains("Texture2D<float4> INPUT : register(t0);"));
        assert!(out.contains("__MP_SwapRB(INPUT.SampleLevel(sam, pos, 0))"));
        assert!(out.contains("__MP_SwapRB(INPUT[gxy])"));
        assert!(out.contains("INPUT.GatherBlue(sam, pos)"));
        assert!(out.contains("OUTPUT[gxy] = __MP_SwapRB(a + b);"));
        assert!(out.contains("INPUT.GetDimensions(w, w)"));
        assert!(out.contains("float MY_INPUT = 1; s.INPUT = 2;"));
    }

    #[test]
    fn only_requested_sides() {
        let (out, i, o) = rewrite("OUTPUT[p] = INPUT[p];", true, false).unwrap();
        assert!(i && !o);
        assert!(out.ends_with("OUTPUT[p] = __MP_SwapRB(INPUT[p]);"));
        let (out, i, o) = rewrite("float x = 1;", true, true).unwrap();
        assert!(!i && !o);
        assert_eq!(out, "float x = 1;");
    }

    #[test]
    fn unsupported_uses() {
        assert!(rewrite("Fxaa(INPUT, pos);", true, false).is_err());
        assert!(rewrite("OUTPUT[p] += 1;", false, true).is_err());
        assert!(rewrite("x = OUTPUT[p];", false, true).is_err());
        assert!(rewrite("INPUT.Foo(1);", true, false).is_err());
    }

    #[test]
    fn swap_pass_is_an_effect() {
        let desc = crate::parser::parse_effect(swap_rb_source(), "SwapRB").unwrap();
        assert_eq!(desc.passes.len(), 1);
        assert_eq!(desc.output().width_expr, "INPUT_WIDTH");
    }
}
