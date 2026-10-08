//! Inlines references to a constant string-lookup array back to the strings.
//!
//! A common obfuscation hoists every string literal into one array and replaces
//! each use with an indexed read:
//!
//! ```js
//! const s = ["log", "hello"];
//! console[s[0]](s[1]);
//! ```
//!
//! This rule recognizes a top-level array whose elements are all string
//! literals, and replaces each `name[<integer literal>]` read with the
//! corresponding string, so the above becomes `console.log("hello")` (the
//! remaining bracket access is simplified by a later rule).
//!
//! ## Safety
//!
//! The contents can only be trusted if the array is never mutated or aliased, so
//! the whole array is left untouched when its binding is used as anything other
//! than an indexed read: reassigned (`name = …`), element-written
//! (`name[i] = …`), method-called (`name.push(…)` — the rotation trick some
//! obfuscators use), static-membered (`name.length`), or passed/aliased
//! (`f(name)`, `const a = name`). A non-literal or out-of-range index is left as
//! is, and an array with any non-string element is not a candidate.

use swc_core::common::{Mark, SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    AssignTarget, Decl, Expr, Lit, MemberExpr, MemberProp, Module, ModuleItem, Pat,
    SimpleAssignTarget, Stmt, Str,
};
use swc_core::ecma::visit::{Visit, VisitMut, VisitMutWith, VisitWith};

use crate::collections::{HashMap, HashSet};

type BindingId = (swc_core::atoms::Atom, SyntaxContext);

pub(crate) fn run(module: &mut Module, _unresolved_mark: Mark) {
    // 1. Collect candidate top-level `NAME = [string literals]` arrays.
    let mut candidates: HashMap<BindingId, Vec<String>> = HashMap::default();
    for item in &module.body {
        let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var))) = item else {
            continue;
        };
        let [decl] = var.decls.as_slice() else {
            continue;
        };
        let Pat::Ident(name) = &decl.name else {
            continue;
        };
        if let Some(values) = string_array_values(decl.init.as_deref()) {
            candidates.insert((name.id.sym.clone(), name.id.ctxt), values);
        }
    }
    if candidates.is_empty() {
        return;
    }

    // 2. Drop any candidate whose binding is used as anything other than an
    //    indexed read (mutation / alias / escape would make the contents or
    //    identity untrustworthy).
    let mut classifier = Classifier {
        candidates: &candidates,
        total: HashMap::default(),
        indexed_reads: HashMap::default(),
        unsafe_bindings: HashSet::default(),
    };
    module.visit_with(&mut classifier);
    let Classifier {
        total,
        indexed_reads,
        unsafe_bindings,
        ..
    } = classifier;

    let safe: HashMap<BindingId, Vec<String>> = candidates
        .into_iter()
        .filter(|(id, _)| {
            // Every reference must be an indexed read. The binding's own
            // declaration name also counts toward `total`, so a safe array has
            // exactly one more total occurrence than indexed reads.
            !unsafe_bindings.contains(id)
                && total.get(id).copied().unwrap_or(0)
                    == indexed_reads.get(id).copied().unwrap_or(0) + 1
        })
        .collect();
    if safe.is_empty() {
        return;
    }

    // 3. Replace `name[<int literal in range>]` with the string.
    let mut folder = Folder {
        arrays: &safe,
        kept: HashSet::default(),
    };
    module.visit_mut_with(&mut folder);

    // 4. Drop the lookup array when every reference was inlined. An array that
    //    still has a non-literal or out-of-range index left is kept.
    let consumed: HashSet<BindingId> = safe
        .keys()
        .filter(|id| !folder.kept.contains(id))
        .cloned()
        .collect();
    if consumed.is_empty() {
        return;
    }
    module
        .body
        .retain(|item| !is_consumed_array_decl(item, &consumed));
}

/// Whether a top-level item is the declaration of a now-unused lookup array.
fn is_consumed_array_decl(item: &ModuleItem, consumed: &HashSet<BindingId>) -> bool {
    let ModuleItem::Stmt(Stmt::Decl(Decl::Var(var))) = item else {
        return false;
    };
    let [decl] = var.decls.as_slice() else {
        return false;
    };
    let Pat::Ident(name) = &decl.name else {
        return false;
    };
    consumed.contains(&(name.id.sym.clone(), name.id.ctxt))
}

/// The string values of an `init` that is an array of only string literals.
fn string_array_values(init: Option<&Expr>) -> Option<Vec<String>> {
    let Some(Expr::Array(array)) = init else {
        return None;
    };
    let mut values = Vec::with_capacity(array.elems.len());
    for elem in &array.elems {
        let Some(elem) = elem else {
            return None; // a hole
        };
        if elem.spread.is_some() {
            return None;
        }
        match elem.expr.as_ref() {
            Expr::Lit(Lit::Str(s)) => values.push(s.value.as_str()?.to_owned()),
            _ => return None,
        }
    }
    Some(values)
}

/// A non-negative integer index `name[<n>]` within range.
fn literal_index(prop: &MemberProp, len: usize) -> Option<usize> {
    let MemberProp::Computed(computed) = prop else {
        return None;
    };
    let Expr::Lit(Lit::Num(num)) = computed.expr.as_ref() else {
        return None;
    };
    if num.value < 0.0 || num.value.fract() != 0.0 {
        return None;
    }
    let index = num.value as usize;
    (index < len).then_some(index)
}

/// The base identifier a write targets, e.g. `a` in `a`, `a[i]`, `a.b`.
fn assign_target_base(target: &AssignTarget) -> Option<BindingId> {
    let AssignTarget::Simple(simple) = target else {
        return None;
    };
    match simple {
        SimpleAssignTarget::Ident(ident) => Some((ident.id.sym.clone(), ident.id.ctxt)),
        SimpleAssignTarget::Member(member) => member_base(member),
        _ => None,
    }
}

fn member_base(member: &MemberExpr) -> Option<BindingId> {
    match member.obj.as_ref() {
        Expr::Ident(ident) => Some((ident.sym.clone(), ident.ctxt)),
        Expr::Member(inner) => member_base(inner),
        _ => None,
    }
}

/// Counts, per candidate binding, how many times it appears in total versus how
/// many of those are indexed-read members (`name[expr]`), and records bindings
/// written through an assignment target.
struct Classifier<'a> {
    candidates: &'a HashMap<BindingId, Vec<String>>,
    total: HashMap<BindingId, usize>,
    indexed_reads: HashMap<BindingId, usize>,
    unsafe_bindings: HashSet<BindingId>,
}

impl Visit for Classifier<'_> {
    fn visit_ident(&mut self, ident: &swc_core::ecma::ast::Ident) {
        let id = (ident.sym.clone(), ident.ctxt);
        if self.candidates.contains_key(&id) {
            *self.total.entry(id).or_insert(0) += 1;
        }
    }

    fn visit_member_expr(&mut self, member: &MemberExpr) {
        if let Expr::Ident(obj) = member.obj.as_ref() {
            let id = (obj.sym.clone(), obj.ctxt);
            if self.candidates.contains_key(&id) && matches!(member.prop, MemberProp::Computed(_)) {
                *self.indexed_reads.entry(id).or_insert(0) += 1;
            }
        }
        member.visit_children_with(self);
    }

    fn visit_assign_expr(&mut self, assign: &swc_core::ecma::ast::AssignExpr) {
        if let Some(base) = assign_target_base(&assign.left) {
            if self.candidates.contains_key(&base) {
                self.unsafe_bindings.insert(base);
            }
        }
        assign.visit_children_with(self);
    }
}

struct Folder<'a> {
    arrays: &'a HashMap<BindingId, Vec<String>>,
    /// Bindings with at least one index that could not be inlined, so their
    /// declaration must stay.
    kept: HashSet<BindingId>,
}

impl VisitMut for Folder<'_> {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        let Expr::Member(member) = &*expr else {
            return;
        };
        let Expr::Ident(obj) = member.obj.as_ref() else {
            return;
        };
        let id = (obj.sym.clone(), obj.ctxt);
        let Some(values) = self.arrays.get(&id) else {
            return;
        };
        let Some(index) = literal_index(&member.prop, values.len()) else {
            // A computed read we cannot resolve (non-literal or out of range):
            // leave it, and keep the array it reads from.
            self.kept.insert(id);
            return;
        };
        *expr = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: values[index].as_str().into(),
            raw: None,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::run;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, Globals, Mark, SourceMap, GLOBALS};
    use swc_core::ecma::codegen::text_writer::JsWriter;
    use swc_core::ecma::codegen::Emitter;
    use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
    use swc_core::ecma::transforms::base::resolver;
    use swc_core::ecma::visit::VisitMutWith;

    /// Parse, resolve, run the rule, and print — isolating the rule from the
    /// rest of the pipeline so only its own rewrite is observed.
    fn run_rule(source: &str) -> String {
        GLOBALS.set(&Globals::new(), || {
            let cm: Lrc<SourceMap> = Default::default();
            let fm = cm.new_source_file(
                FileName::Custom("test.js".into()).into(),
                source.to_string(),
            );
            let lexer = Lexer::new(
                Syntax::Es(EsSyntax::default()),
                Default::default(),
                StringInput::from(&*fm),
                None,
            );
            let mut module = Parser::new_from(lexer).parse_module().expect("parse");
            let unresolved_mark = Mark::new();
            module.visit_mut_with(&mut resolver(unresolved_mark, Mark::new(), false));

            run(&mut module, unresolved_mark);

            let mut buf = Vec::new();
            {
                let mut emitter = Emitter {
                    cfg: Default::default(),
                    cm: cm.clone(),
                    comments: None,
                    wr: JsWriter::new(cm.clone(), "\n", &mut buf, None),
                };
                emitter.emit_module(&module).expect("emit");
            }
            String::from_utf8(buf).unwrap()
        })
    }

    #[test]
    fn inlines_indexed_reads() {
        let out = run_rule(r#"const s = ["log", "hello"]; console[s[0]](s[1]);"#);
        assert!(out.contains(r#"console["log"]("hello")"#), "{out}");
    }

    #[test]
    fn removes_array_when_fully_consumed() {
        let out = run_rule(r#"const s = ["a", "b"]; f(s[0], s[1]);"#);
        assert!(!out.contains("const s"), "array should be dropped: {out}");
        assert!(out.contains(r#"f("a", "b")"#), "{out}");
    }

    #[test]
    fn keeps_array_with_unresolved_index() {
        let out = run_rule(r#"const s = ["x", "y"]; f(s[0], s[i]);"#);
        assert!(out.contains("const s"), "array must stay: {out}");
        assert!(out.contains("s[i]"), "dynamic index must stay: {out}");
        assert!(out.contains(r#"f("x", s[i])"#), "{out}");
    }

    #[test]
    fn ignores_out_of_range_index() {
        let out = run_rule(r#"const s = ["x"]; f(s[5]);"#);
        assert!(out.contains("s[5]"), "out-of-range index must stay: {out}");
    }

    #[test]
    fn ignores_reassigned_array() {
        let out = run_rule(r#"const s = ["x"]; s = y; f(s[0]);"#);
        assert!(
            out.contains("s[0]"),
            "reassigned array must not fold: {out}"
        );
    }

    #[test]
    fn ignores_mutating_method_call() {
        let out = run_rule(r#"const s = ["x", "y"]; s.push("z"); f(s[0]);"#);
        assert!(out.contains("s[0]"), "mutated array must not fold: {out}");
    }

    #[test]
    fn ignores_aliased_array() {
        let out = run_rule(r#"const s = ["x"]; const t = s; f(t[0]);"#);
        assert!(out.contains("t[0]"), "aliased array must not fold: {out}");
    }

    #[test]
    fn ignores_non_string_element() {
        let out = run_rule(r#"const s = ["x", 1]; f(s[0]);"#);
        assert!(
            out.contains("s[0]"),
            "mixed array is not a candidate: {out}"
        );
    }

    #[test]
    fn only_folds_top_level_binding() {
        let out = run_rule(
            r#"const s = ["outer"]; function f() { const s = ["inner"]; return s[0]; } g(s[0]);"#,
        );
        assert!(out.contains(r#"g("outer")"#), "{out}");
        assert!(out.contains("s[0]"), "inner shadowed read must stay: {out}");
        assert!(out.contains(r#""inner""#), "inner array must stay: {out}");
    }
}
