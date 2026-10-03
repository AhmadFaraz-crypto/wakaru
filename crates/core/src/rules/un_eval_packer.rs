//! Decodes Dean Edwards' `packer` output back into readable source.
//!
//! The `packer` tool (<http://dean.edwards.name/packer/>, ~2004-2007) wraps a
//! program in a self-extracting `eval` call:
//!
//! ```js
//! eval(function(p,a,c,k,e,r){e=function(c){return(c<a?'':e(parseInt(c/a)))+
//! ((c=c%a)>35?String.fromCharCode(c+29):c.toString(36))};if(!''.replace(/^/,
//! String)){while(c--)r[e(c)]=k[c]||e(c);k=[function(e){return r[e]}];e=
//! function(){return'\\w+'};c=1};while(c--)if(k[c])p=p.replace(new RegExp(
//! '\\b'+e(c)+'\\b','g'),k[c]);return p}('PAYLOAD',RADIX,COUNT,
//! 'word0|word1|...'.split('|'),0,{}))
//! ```
//!
//! At runtime the wrapper rebuilds the original source: every `\b<token>\b` in
//! `PAYLOAD` is a base-`RADIX` index into the `|`-separated dictionary, and is
//! replaced by the dictionary word (empty dictionary slots keep the literal
//! token). This rule recognizes a top-level `eval(...)` statement with that
//! exact wrapper shape, performs the same substitution statically, re-parses the
//! recovered source, and splices it in place of the `eval` call.
//!
//! Only the canonical six-parameter `function(p,a,c,k,e,r)` wrapper invoked with
//! a string payload, numeric radix and count, a `'...'.split('|')` dictionary,
//! `0`, and `{}` is decoded. Anything else — a shadowed local `eval`, a plain
//! `eval("source")`, a different argument shape, or a payload that does not
//! re-parse as valid JavaScript — is left untouched.

use swc_core::common::{sync::Lrc, FileName, Mark, SourceMap};
use swc_core::ecma::ast::{
    CallExpr, Callee, Expr, ExprStmt, FnExpr, Lit, MemberProp, Module, ModuleItem, Pat, Stmt,
};
use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::visit::{Visit, VisitMutWith, VisitWith};

/// The fixed parameter names of the packer wrapper, in order.
const PACKER_PARAMS: [&str; 6] = ["p", "a", "c", "k", "e", "r"];

/// Decode and splice every top-level packer `eval(...)` statement in `module`.
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

/// If `item` is a top-level packer `eval(...)` expression statement, decode it
/// and return the recovered module items; otherwise `None`.
fn decode_item(item: &ModuleItem, unresolved_mark: Mark) -> Option<Vec<ModuleItem>> {
    let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item else {
        return None;
    };
    let args = extract_packer_args(expr, unresolved_mark)?;
    let source = decode_packer(&args.payload, args.radix, args.count, &args.dict);
    let recovered = parse_module(&source, unresolved_mark)?;
    Some(recovered)
}

struct PackerArgs {
    payload: String,
    radix: usize,
    count: usize,
    dict: Vec<String>,
}

/// Match `eval(function(p,a,c,k,e,r){...}(payload, radix, count,
/// dict.split('|'), 0, {}))` and pull out the four data arguments.
fn extract_packer_args(expr: &Expr, unresolved_mark: Mark) -> Option<PackerArgs> {
    // Outer call must be a direct, global `eval(...)` with a single argument.
    let Expr::Call(eval_call) = expr else {
        return None;
    };
    let Callee::Expr(callee) = &eval_call.callee else {
        return None;
    };
    let Expr::Ident(eval_ident) = callee.as_ref() else {
        return None;
    };
    if eval_ident.sym != "eval" || eval_ident.ctxt.outer() != unresolved_mark {
        return None;
    }
    let [eval_arg] = eval_call.args.as_slice() else {
        return None;
    };
    if eval_arg.spread.is_some() {
        return None;
    }

    // The argument is an IIFE: `function(p,a,c,k,e,r){...}(...)`.
    let Expr::Call(iife) = eval_arg.expr.as_ref() else {
        return None;
    };
    let Callee::Expr(iife_callee) = &iife.callee else {
        return None;
    };
    let Expr::Fn(fn_expr) = iife_callee.as_ref() else {
        return None;
    };
    if !has_packer_params(&fn_expr.function.params) {
        return None;
    }
    // The parameter names alone are not proof: the body must actually perform
    // the token substitution (`p.replace(...)`). This rejects an unrelated
    // six-parameter `(p,a,c,k,e,r)` function that happens to share the shape.
    if !body_has_substitution(fn_expr) {
        return None;
    }

    // Arguments: payload, radix, count, dict.split('|'), 0, {}.
    let [payload, radix, count, dict, zero, store] = iife.args.as_slice() else {
        return None;
    };
    if [payload, radix, count, dict, zero, store]
        .iter()
        .any(|arg| arg.spread.is_some())
    {
        return None;
    }
    let payload = string_lit(&payload.expr)?;
    let radix = int_lit(&radix.expr)?;
    let count = int_lit(&count.expr)?;
    let dict = split_dict(&dict.expr)?;
    // The trailing `0` seed and `{}` lookup object pin the canonical shape.
    if int_lit(&zero.expr)? != 0 || !is_empty_object(&store.expr) {
        return None;
    }
    // A radix outside the base-62 alphabet (`0-9a-zA-Z`) is not this format.
    if !(2..=62).contains(&radix) {
        return None;
    }

    Some(PackerArgs {
        payload,
        radix,
        count,
        dict,
    })
}

fn has_packer_params(params: &[swc_core::ecma::ast::Param]) -> bool {
    if params.len() != PACKER_PARAMS.len() {
        return false;
    }
    params
        .iter()
        .zip(PACKER_PARAMS)
        .all(|(param, name)| matches!(&param.pat, Pat::Ident(ident) if ident.id.sym == name))
}

/// Whether the wrapper body contains a `.replace(...)` call — the substitution
/// step every packer variant performs to rebuild the source.
fn body_has_substitution(fn_expr: &FnExpr) -> bool {
    struct ReplaceFinder {
        found: bool,
    }
    impl Visit for ReplaceFinder {
        fn visit_call_expr(&mut self, call: &CallExpr) {
            if let Callee::Expr(callee) = &call.callee {
                if let Expr::Member(member) = callee.as_ref() {
                    if matches!(&member.prop, MemberProp::Ident(name) if name.sym == "replace") {
                        self.found = true;
                    }
                }
            }
            call.visit_children_with(self);
        }
    }
    let mut finder = ReplaceFinder { found: false };
    fn_expr.function.visit_with(&mut finder);
    finder.found
}

fn string_lit(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(Lit::Str(s)) => s.value.as_str().map(str::to_owned),
        _ => None,
    }
}

/// A non-negative integer numeric literal, as `usize`.
fn int_lit(expr: &Expr) -> Option<usize> {
    let Expr::Lit(Lit::Num(num)) = expr else {
        return None;
    };
    if num.value < 0.0 || num.value.fract() != 0.0 {
        return None;
    }
    Some(num.value as usize)
}

fn is_empty_object(expr: &Expr) -> bool {
    matches!(expr, Expr::Object(obj) if obj.props.is_empty())
}

/// Match `"<words>".split('|')` and return the split words.
fn split_dict(expr: &Expr) -> Option<Vec<String>> {
    let Expr::Call(call) = expr else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(member) = callee.as_ref() else {
        return None;
    };
    let MemberProp::Ident(method) = &member.prop else {
        return None;
    };
    if method.sym != "split" {
        return None;
    }
    let words = string_lit(&member.obj)?;
    let [sep] = call.args.as_slice() else {
        return None;
    };
    if sep.spread.is_some() || string_lit(&sep.expr)? != "|" {
        return None;
    }
    Some(words.split('|').map(str::to_string).collect())
}

/// Statically reproduce the packer substitution: for each dictionary index from
/// `count - 1` down to `0`, replace whole-word occurrences of its base-`radix`
/// token with the dictionary word. Empty slots are left as their literal token.
fn decode_packer(payload: &str, radix: usize, count: usize, dict: &[String]) -> String {
    let mut source = payload.to_string();
    for index in (0..count).rev() {
        let Some(word) = dict.get(index) else {
            continue;
        };
        if word.is_empty() {
            continue;
        }
        let token = encode_token(index, radix);
        source = replace_whole_word(&source, &token, word);
    }
    source
}

/// Encode `index` in base `radix` using the packer alphabet `0-9a-zA-Z`,
/// most-significant digit first and without leading zeros (matching the
/// wrapper's `e(c)`).
fn encode_token(index: usize, radix: usize) -> String {
    fn digit(d: usize) -> char {
        let byte = if d < 10 {
            b'0' + d as u8
        } else if d < 36 {
            b'a' + (d - 10) as u8
        } else {
            b'A' + (d - 36) as u8
        };
        byte as char
    }
    if index < radix {
        return digit(index).to_string();
    }
    let mut digits = Vec::new();
    let mut n = index;
    while n > 0 {
        digits.push(digit(n % radix));
        n /= radix;
    }
    digits.iter().rev().collect()
}

/// Whether `byte` is an ASCII word character (`[A-Za-z0-9_]`), matching `\w` in
/// the wrapper's `\b...\b` regexp. Non-ASCII bytes are never word characters.
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Replace every `\b<token>\b` occurrence in `haystack` with `replacement`.
/// `token` is ASCII; the boundary check matches JavaScript `\b` against ASCII
/// `\w`. Non-matching text is copied as whole UTF-8 slices.
fn replace_whole_word(haystack: &str, token: &str, replacement: &str) -> String {
    let bytes = haystack.as_bytes();
    let token_bytes = token.as_bytes();
    let mut out = String::with_capacity(haystack.len());
    let mut last = 0;
    let mut i = 0;
    while i < bytes.len() {
        let matches_here = bytes[i..].starts_with(token_bytes) && {
            let before_ok = i == 0 || !is_word_byte(bytes[i - 1]);
            let after = i + token_bytes.len();
            let after_ok = after >= bytes.len() || !is_word_byte(bytes[after]);
            before_ok && after_ok
        };
        if matches_here {
            out.push_str(&haystack[last..i]);
            out.push_str(replacement);
            i += token_bytes.len();
            last = i;
        } else {
            // Advance by one whole UTF-8 character to never split a codepoint.
            i += utf8_len(bytes[i]);
        }
    }
    out.push_str(&haystack[last..]);
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// Parse recovered source into resolved module items, or `None` if it does not
/// parse. A fresh `top_level` mark keeps the fragment's bindings distinct while
/// `unresolved_mark` lets later rules recognize its free/global identifiers.
fn parse_module(source: &str, unresolved_mark: Mark) -> Option<Vec<ModuleItem>> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        FileName::Custom("packer-decoded.js".into()).into(),
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
    use super::{decode_packer, encode_token, replace_whole_word};

    #[test]
    fn encodes_base62_tokens() {
        assert_eq!(encode_token(0, 62), "0");
        assert_eq!(encode_token(9, 62), "9");
        assert_eq!(encode_token(10, 62), "a");
        assert_eq!(encode_token(35, 62), "z");
        assert_eq!(encode_token(36, 62), "A");
        assert_eq!(encode_token(61, 62), "Z");
        assert_eq!(encode_token(62, 62), "10");
    }

    #[test]
    fn encodes_low_radix_tokens() {
        assert_eq!(encode_token(0, 36), "0");
        assert_eq!(encode_token(36, 36), "10");
    }

    #[test]
    fn decodes_simple_payload() {
        let dict: Vec<String> = ["function", "foo", "return", "bar"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            decode_packer("0 1(){2 3}", 62, 4, &dict),
            "function foo(){return bar}"
        );
    }

    #[test]
    fn keeps_empty_dictionary_slots_as_tokens() {
        let dict: Vec<String> = ["let", "", "x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(decode_packer("0 1 2", 36, 3, &dict), "let 1 x");
    }

    #[test]
    fn word_boundary_does_not_match_inside_identifiers() {
        // Token `a` must not be replaced inside `bar` or `cab`.
        assert_eq!(replace_whole_word("a bar cab a", "a", "X"), "X bar cab X");
    }

    #[test]
    fn preserves_non_ascii_between_tokens() {
        let dict: Vec<String> = ["x"].iter().map(|s| s.to_string()).collect();
        assert_eq!(
            decode_packer("\"\u{00e9}\" 0", 62, 1, &dict),
            "\"\u{00e9}\" x"
        );
    }
}
