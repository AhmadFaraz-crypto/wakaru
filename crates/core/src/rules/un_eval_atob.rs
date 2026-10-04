//! Recovers source hidden behind `eval(atob("<base64>"))`.
//!
//! A common obfuscation encodes a program as base64 and runs it through
//! `eval(atob(...))`:
//!
//! ```js
//! eval(atob("YWxlcnQoMSk=")) // -> alert(1)
//! ```
//!
//! This rule recognizes a top-level `eval(atob("..."))` call where both `eval`
//! and `atob` are the globals, decodes the base64 string the same way `atob`
//! does (standard alphabet, `=` padding, ASCII whitespace ignored; each decoded
//! byte becomes one Latin-1 code unit), re-parses the result, and splices it in
//! place of the `eval` call.
//!
//! It is left untouched when it is not this exact shape — a shadowed local
//! `eval`/`atob`, a non-string `atob` argument, `eval` of anything other than
//! `atob(...)`, base64 that is not decodable, or a decoded string that does not
//! re-parse as valid JavaScript.

use swc_core::common::{sync::Lrc, FileName, Mark, SourceMap};
use swc_core::ecma::ast::{Callee, Expr, ExprStmt, Lit, Module, ModuleItem, Stmt};
use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::visit::VisitMutWith;

/// Decode and splice every top-level `eval(atob("..."))` statement.
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
    let base64 = extract_eval_atob(expr, unresolved_mark)?;
    let source = base64_decode_to_latin1(&base64)?;
    parse_module(&source, unresolved_mark)
}

/// Match `eval(atob(<string literal>))` with both callees the globals, and
/// return the base64 string.
fn extract_eval_atob(expr: &Expr, unresolved_mark: Mark) -> Option<String> {
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

    let Expr::Call(atob_call) = eval_arg.expr.as_ref() else {
        return None;
    };
    if !is_global_call(&atob_call.callee, "atob", unresolved_mark) {
        return None;
    }
    let [atob_arg] = atob_call.args.as_slice() else {
        return None;
    };
    if atob_arg.spread.is_some() {
        return None;
    }
    string_lit(&atob_arg.expr)
}

/// Whether `callee` is the global function named `name` (not a shadowing local).
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

/// Decode standard base64 the way `atob` does: skip ASCII whitespace, stop at
/// padding, reject invalid characters and lengths. Each decoded byte becomes one
/// Latin-1 code unit (so the result matches `atob`'s binary string). Returns
/// `None` for input `atob` would reject.
fn base64_decode_to_latin1(input: &str) -> Option<String> {
    fn sextet(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a') as u32 + 26),
            b'0'..=b'9' => Some((byte - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|b| !matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .collect();
    if cleaned.len() % 4 == 1 {
        return None;
    }

    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    let mut out = String::new();
    for &byte in &cleaned {
        if byte == b'=' {
            break;
        }
        let value = sextet(byte)?;
        bits = (bits << 6) | value;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((((bits >> nbits) & 0xff) as u8) as char);
        }
    }
    Some(out)
}

/// Parse recovered source into resolved module items, or `None` if it does not
/// parse.
fn parse_module(source: &str, unresolved_mark: Mark) -> Option<Vec<ModuleItem>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("atob-decoded.js".into()).into(),
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
    use super::base64_decode_to_latin1;

    #[test]
    fn decodes_ascii() {
        assert_eq!(
            base64_decode_to_latin1("YWxlcnQoMSk=").as_deref(),
            Some("alert(1)")
        );
    }

    #[test]
    fn ignores_whitespace() {
        assert_eq!(
            base64_decode_to_latin1("YW xl cnQo MSk=").as_deref(),
            Some("alert(1)")
        );
    }

    #[test]
    fn accepts_missing_padding() {
        assert_eq!(base64_decode_to_latin1("aGk").as_deref(), Some("hi"));
    }

    #[test]
    fn rejects_invalid_character() {
        assert_eq!(base64_decode_to_latin1("###"), None);
    }

    #[test]
    fn rejects_bad_length() {
        assert_eq!(base64_decode_to_latin1("YWxlcnQoMSk=A"), None);
    }
}
