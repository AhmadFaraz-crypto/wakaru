//! Recovers source hidden behind `eval(unescape("..."))`.
//!
//! A classic obfuscation percent-encodes a program and runs it through
//! `eval(unescape(...))`:
//!
//! ```js
//! eval(unescape("alert(%22hi%22)")) // -> alert("hi")
//! ```
//!
//! This rule recognizes a top-level `eval(unescape("..."))` call where both
//! `eval` and `unescape` are the globals, decodes the string exactly as
//! `unescape` does (`%XX` -> one code unit, `%uXXXX` -> one code unit, every
//! other character and any malformed `%` escape left as-is), re-parses the
//! result, and splices it in place of the `eval` call.
//!
//! It is left untouched when it is not this exact shape — a shadowed local
//! `eval`/`unescape`, a non-string `unescape` argument, `eval` of anything
//! other than `unescape(...)`, extra arguments, or a decoded string that does
//! not re-parse as valid JavaScript (for example a `%uXXXX` lone surrogate).

use swc_core::common::{sync::Lrc, FileName, Mark, SourceMap};
use swc_core::ecma::ast::{Callee, Expr, ExprStmt, Lit, Module, ModuleItem, Stmt};
use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::visit::VisitMutWith;

/// Decode and splice every top-level `eval(unescape("..."))` statement.
pub(crate) fn run(module: &mut Module, unresolved_mark: Mark) {
    if module.body.is_empty() {
        return;
    }
    let original = std::mem::take(&mut module.body);
    let mut out: Vec<ModuleItem> = Vec::with_capacity(original.len());
    for item in original {
        match decode_item(&item, unresolved_mark) {
            Some(decoded) => out.extend(decoded),
            None => out.push(item),
        }
    }
    module.body = out;
}

fn decode_item(item: &ModuleItem, unresolved_mark: Mark) -> Option<Vec<ModuleItem>> {
    let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item else {
        return None;
    };
    let encoded = extract_eval_unescape(expr, unresolved_mark)?;
    let source = unescape_decode(&encoded)?;
    parse_module(&source, unresolved_mark)
}

/// Match `eval(unescape(<string literal>))` with both callees the globals, and
/// return the encoded string.
fn extract_eval_unescape(expr: &Expr, unresolved_mark: Mark) -> Option<String> {
    let Expr::Call(eval_call) = expr else {
        return None;
    };
    if !is_global_call(&eval_call.callee, "eval", unresolved_mark) {
        return None;
    }
    let [eval_arg] = eval_call.args.as_slice() else {
        return None;
    };
    if eval_arg.spread.is_some() {
        return None;
    }

    let Expr::Call(unescape_call) = eval_arg.expr.as_ref() else {
        return None;
    };
    if !is_global_call(&unescape_call.callee, "unescape", unresolved_mark) {
        return None;
    }
    let [unescape_arg] = unescape_call.args.as_slice() else {
        return None;
    };
    if unescape_arg.spread.is_some() {
        return None;
    }
    string_lit(&unescape_arg.expr)
}

fn is_global_call(callee: &Callee, name: &str, unresolved_mark: Mark) -> bool {
    let Callee::Expr(expr) = callee else {
        return false;
    };
    matches!(expr.as_ref(), Expr::Ident(ident)
        if ident.sym == name && ident.ctxt.outer() == unresolved_mark)
}

fn string_lit(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().map(str::to_owned),
        _ => None,
    }
}

/// Decode the way `unescape` does: `%XX` and `%uXXXX` become single UTF-16 code
/// units, every other character contributes its own code units, and a malformed
/// `%` escape is kept literally. Returns `None` when the resulting code-unit
/// sequence is not valid UTF-16 (e.g. a `%uXXXX` lone surrogate).
fn unescape_decode(input: &str) -> Option<String> {
    fn hex_value(bytes: &[u8]) -> Option<u16> {
        let mut value: u16 = 0;
        for &byte in bytes {
            let digit = (byte as char).to_digit(16)?;
            value = value * 16 + digit as u16;
        }
        Some(value)
    }

    let bytes = input.as_bytes();
    let mut units: Vec<u16> = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if bytes.get(i + 1) == Some(&b'u') {
                if let Some(unit) = bytes.get(i + 2..i + 6).and_then(hex_value) {
                    units.push(unit);
                    i += 6;
                    continue;
                }
            } else if let Some(unit) = bytes.get(i + 1..i + 3).and_then(hex_value) {
                units.push(unit);
                i += 3;
                continue;
            }
            // Malformed escape: keep the `%` literally.
            units.push(b'%' as u16);
            i += 1;
        } else {
            // ASCII byte or the lead byte of a UTF-8 sequence: decode one char.
            let ch = input[i..].chars().next().unwrap();
            let mut buf = [0u16; 2];
            units.extend_from_slice(ch.encode_utf16(&mut buf));
            i += ch.len_utf8();
        }
    }

    String::from_utf16(&units).ok()
}

fn parse_module(source: &str, unresolved_mark: Mark) -> Option<Vec<ModuleItem>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("unescape-decoded.js".into()).into(),
        source.to_string(),
    );
    let lexer = Lexer::new(
        Syntax::Es(EsSyntax::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );
    let mut parser = Parser::new_from(lexer);
    let mut module = parser.parse_module().ok()?;
    if !parser.take_errors().is_empty() {
        return None;
    }
    module.visit_mut_with(&mut resolver(unresolved_mark, Mark::new(), false));
    Some(module.body)
}

#[cfg(test)]
mod tests {
    use super::unescape_decode;

    #[test]
    fn decodes_percent_hex() {
        assert_eq!(unescape_decode("%66%6f%6f").as_deref(), Some("foo"));
    }

    #[test]
    fn decodes_mixed_literal_and_escape() {
        assert_eq!(
            unescape_decode("alert(%22hi%22)").as_deref(),
            Some("alert(\"hi\")")
        );
    }

    #[test]
    fn decodes_percent_u() {
        assert_eq!(unescape_decode("%u0041").as_deref(), Some("A"));
    }

    #[test]
    fn keeps_malformed_escape() {
        assert_eq!(unescape_decode("a%zz").as_deref(), Some("a%zz"));
        assert_eq!(unescape_decode("100%").as_deref(), Some("100%"));
    }

    #[test]
    fn rejects_lone_surrogate() {
        assert_eq!(unescape_decode("%uD83D"), None);
    }
}
