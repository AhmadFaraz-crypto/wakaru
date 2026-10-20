//! Folds constant numeric expressions back to the value they evaluate to.
//!
//! JavaScript obfuscators (for example obfuscator.io's "Numbers To
//! Expressions") hide a plain number behind arithmetic over literals such as
//! `0x1a + 0x5` or `0x45 ^ 0x4e`. This rule evaluates such an expression and
//! replaces it with the single number ECMAScript would compute, so `0x1a + 0x5`
//! becomes `31` and `~0xff` becomes `-256`.
//!
//! Folding is intentionally restricted to the obfuscation shape: a constant
//! subtree is only collapsed when it contains a bitwise/shift/tilde operator or
//! a literal written in a non-decimal notation (hex/octal/binary). Plain
//! decimal arithmetic like `60 * 1000` is left untouched, because the factored
//! form is usually the more readable one.
//!
//! Operands fold with ECMAScript value semantics. A subtree of number literals
//! — with `boolean` and `null` coerced to numbers as ECMAScript does, so
//! `true & 0x1` is `1` — folds to a `Number`, and only a finite result is kept
//! (`Infinity` or `NaN`, such as a division by zero, has no clean literal
//! spelling and is left as written). A subtree made entirely of `BigInt`
//! literals folds to a `BigInt` (`0x1n + 0x2n` is `3n`); `Number` and `BigInt`
//! are never mixed, since ECMAScript throws on that. An identifier or string
//! operand makes the subtree non-constant, and an operator that evaluates to a
//! boolean (a comparison or logical op) is not a numeric fold and is left.

use swc_core::common::DUMMY_SP;
use swc_core::ecma::ast::{BigInt, BinExpr, BinaryOp, Expr, Lit, Number, UnaryExpr, UnaryOp};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

pub struct FoldConstantExpr;

impl VisitMut for FoldConstantExpr {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        expr.visit_mut_children_with(self);

        // Only an expression that performs a computation can collapse; a bare
        // literal is already minimal and must not be rematched.
        if !matches!(expr, Expr::Bin(_) | Expr::Unary(_) | Expr::Paren(_)) {
            return;
        }

        if let Some(folded) = eval(expr) {
            if folded.marker && folded.value.is_finite() {
                *expr = number_expr(folded.value);
            }
            return;
        }

        if let Some((folded, marker)) = eval_bigint(expr) {
            if marker {
                *expr = Expr::Lit(Lit::BigInt(folded));
            }
        }
    }
}

/// The evaluated value of a constant subtree, plus whether that subtree carries
/// an obfuscation marker (a bitwise/shift/tilde operator, or a non-decimal
/// literal). The marker gate keeps readable decimal arithmetic unfolded.
struct Folded {
    value: f64,
    marker: bool,
}

fn eval(expr: &Expr) -> Option<Folded> {
    match expr {
        Expr::Lit(Lit::Num(num)) => Some(Folded {
            value: num.value,
            marker: raw_is_non_decimal(num.raw.as_deref()),
        }),
        // ECMAScript coerces boolean and null to a number in arithmetic.
        Expr::Lit(Lit::Bool(b)) => Some(Folded {
            value: if b.value { 1.0 } else { 0.0 },
            marker: false,
        }),
        Expr::Lit(Lit::Null(_)) => Some(Folded {
            value: 0.0,
            marker: false,
        }),
        Expr::Paren(paren) => eval(&paren.expr),
        Expr::Unary(unary) => eval_unary(unary),
        Expr::Bin(bin) => eval_bin(bin),
        _ => None,
    }
}

fn eval_unary(unary: &UnaryExpr) -> Option<Folded> {
    let arg = eval(&unary.arg)?;
    match unary.op {
        UnaryOp::Minus => Some(Folded {
            value: -arg.value,
            marker: arg.marker,
        }),
        UnaryOp::Plus => Some(Folded {
            value: arg.value,
            marker: arg.marker,
        }),
        // Bitwise NOT coerces to int32 and is itself an obfuscation marker.
        UnaryOp::Tilde => Some(Folded {
            value: f64::from(!to_int32(arg.value)),
            marker: true,
        }),
        // `!`, `typeof`, `void`, `delete` are not numeric folds.
        _ => None,
    }
}

fn eval_bin(bin: &BinExpr) -> Option<Folded> {
    let left = eval(&bin.left)?;
    let right = eval(&bin.right)?;
    let (l, r) = (left.value, right.value);

    let (value, op_marker) = match bin.op {
        BinaryOp::Add => (l + r, false),
        BinaryOp::Sub => (l - r, false),
        BinaryOp::Mul => (l * r, false),
        BinaryOp::Div => (l / r, false),
        BinaryOp::Mod => (l % r, false),
        BinaryOp::Exp => (l.powf(r), false),
        BinaryOp::BitAnd => (f64::from(to_int32(l) & to_int32(r)), true),
        BinaryOp::BitOr => (f64::from(to_int32(l) | to_int32(r)), true),
        BinaryOp::BitXor => (f64::from(to_int32(l) ^ to_int32(r)), true),
        BinaryOp::LShift => (f64::from(to_int32(l).wrapping_shl(shift_count(r))), true),
        BinaryOp::RShift => (f64::from(to_int32(l) >> shift_count(r)), true),
        BinaryOp::ZeroFillRShift => (f64::from(to_uint32(l) >> shift_count(r)), true),
        // Comparisons, logical, `in`, `instanceof`, `??` are not numeric folds.
        _ => return None,
    };

    Some(Folded {
        value,
        marker: op_marker || left.marker || right.marker,
    })
}

/// Builds the literal expression for a folded value. Negative values (and
/// negative zero) are emitted as a unary minus over a non-negative numeric
/// literal, matching how source spells them and keeping the `Number` node
/// non-negative.
fn number_expr(value: f64) -> Expr {
    if value.is_sign_negative() {
        Expr::Unary(UnaryExpr {
            span: DUMMY_SP,
            op: UnaryOp::Minus,
            arg: Box::new(num_lit(-value)),
        })
    } else {
        num_lit(value)
    }
}

fn num_lit(value: f64) -> Expr {
    Expr::Lit(Lit::Num(Number {
        span: DUMMY_SP,
        value,
        raw: None,
    }))
}

/// Whether a literal's raw spelling uses a non-decimal notation (hex/octal/
/// binary). The raw text survives into this rule because it runs before the
/// pass that strips redundant raw spellings.
fn raw_is_non_decimal(raw: Option<&str>) -> bool {
    raw.is_some_and(|raw| {
        let mut chars = raw.chars();
        chars.next() == Some('0') && matches!(chars.next(), Some('x' | 'X' | 'o' | 'O' | 'b' | 'B'))
    })
}

/// The `BigInt` value of an all-`BigInt` constant subtree (carried as an AST
/// node so the concrete integer type stays internal), plus the obfuscation
/// marker. Returns `None` for any non-`BigInt` operand, so `Number` and
/// `BigInt` never mix.
fn eval_bigint(expr: &Expr) -> Option<(BigInt, bool)> {
    match expr {
        Expr::Lit(Lit::BigInt(b)) => Some((
            BigInt {
                span: DUMMY_SP,
                value: b.value.clone(),
                raw: None,
            },
            raw_is_non_decimal(b.raw.as_deref()),
        )),
        Expr::Paren(paren) => eval_bigint(&paren.expr),
        Expr::Unary(unary) => eval_bigint_unary(unary),
        Expr::Bin(bin) => eval_bigint_bin(bin),
        _ => None,
    }
}

fn eval_bigint_unary(unary: &UnaryExpr) -> Option<(BigInt, bool)> {
    let (arg, marker) = eval_bigint(&unary.arg)?;
    match unary.op {
        UnaryOp::Minus => Some((
            BigInt {
                span: DUMMY_SP,
                value: Box::new(-(*arg.value).clone()),
                raw: None,
            },
            marker,
        )),
        // Unary plus throws on a BigInt; `~`/shifts need int conversions we do
        // not perform for BigInt. Leave them untouched.
        _ => None,
    }
}

fn eval_bigint_bin(bin: &BinExpr) -> Option<(BigInt, bool)> {
    let (left, lmarker) = eval_bigint(&bin.left)?;
    let (right, rmarker) = eval_bigint(&bin.right)?;
    // A zero divisor makes `/` and `%` throw in ECMAScript (and would panic the
    // integer math here), so bail before touching those operators.
    let right_is_zero = format!("{}", right.value) == "0";
    let l = (*left.value).clone();
    let r = (*right.value).clone();

    let (value, op_marker) = match bin.op {
        BinaryOp::Add => (l + r, false),
        BinaryOp::Sub => (l - r, false),
        BinaryOp::Mul => (l * r, false),
        BinaryOp::Div if !right_is_zero => (l / r, false),
        BinaryOp::Mod if !right_is_zero => (l % r, false),
        BinaryOp::BitAnd => (l & r, true),
        BinaryOp::BitOr => (l | r, true),
        BinaryOp::BitXor => (l ^ r, true),
        // Exponentiation, shifts, `>>>`, comparisons, logical, and a zero
        // divisor are all left as written.
        _ => return None,
    };

    Some((
        BigInt {
            span: DUMMY_SP,
            value: Box::new(value),
            raw: None,
        },
        op_marker || lmarker || rmarker,
    ))
}

/// ECMAScript shift count: `ToUint32(rhs) & 31`, always in `0..=31`.
fn shift_count(value: f64) -> u32 {
    to_uint32(value) & 31
}

/// ECMAScript `ToUint32`.
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() || value == 0.0 {
        return 0;
    }
    // `rem_euclid` lands in `[0, 2^32)`, which fits a u32 exactly.
    value.trunc().rem_euclid(4_294_967_296.0) as u32
}

/// ECMAScript `ToInt32`: reinterpret the `ToUint32` bit pattern as signed.
fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

#[cfg(test)]
mod tests {
    use super::FoldConstantExpr;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, Globals, SourceMap, GLOBALS};
    use swc_core::ecma::codegen::text_writer::JsWriter;
    use swc_core::ecma::codegen::Emitter;
    use swc_core::ecma::parser::{lexer::Lexer, EsSyntax, Parser, StringInput, Syntax};
    use swc_core::ecma::visit::VisitMutWith;

    /// Parse, run the rule, and print — isolating this rewrite from the rest of
    /// the pipeline. No resolver pass is needed: the rule matches literals, not
    /// bindings.
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

            module.visit_mut_with(&mut FoldConstantExpr);

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
    fn folds_hex_addition() {
        assert_eq!(run_rule("x = 0x1a + 0x5;").trim(), "x = 31;");
    }

    #[test]
    fn folds_bitwise_xor() {
        assert_eq!(run_rule("x = 0x45 ^ 0x4e;").trim(), "x = 11;");
    }

    #[test]
    fn folds_bitwise_not_to_negative() {
        assert_eq!(run_rule("x = ~0xff;").trim(), "x = -256;");
    }

    #[test]
    fn folds_left_shift_with_decimal_operands() {
        // The shift operator is itself the marker, so decimal operands fold.
        assert_eq!(run_rule("x = 1 << 4;").trim(), "x = 16;");
    }

    #[test]
    fn folds_unsigned_right_shift_to_u32_range() {
        assert_eq!(run_rule("x = -1 >>> 0;").trim(), "x = 4294967295;");
    }

    #[test]
    fn folds_nested_and_parenthesized() {
        assert_eq!(run_rule("x = (0x2 + 0x3) << 1;").trim(), "x = 10;");
    }

    #[test]
    fn folds_inside_member_index() {
        assert_eq!(run_rule("y = arr[0x2 + 0x1];").trim(), "y = arr[3];");
    }

    #[test]
    fn folds_constant_part_but_keeps_string_concat() {
        assert_eq!(
            run_rule(r#"x = 0x1 + 0x2 + "z";"#).trim(),
            r#"x = 3 + "z";"#
        );
    }

    #[test]
    fn keeps_plain_decimal_arithmetic() {
        // No bitwise op and no non-decimal literal: left readable.
        assert_eq!(run_rule("x = 60 * 1000;").trim(), "x = 60 * 1000;");
    }

    #[test]
    fn keeps_non_finite_division() {
        assert_eq!(run_rule("x = 0xff / 0;").trim(), "x = 0xff / 0;");
    }

    #[test]
    fn keeps_non_literal_operand() {
        assert_eq!(run_rule("x = y + 0x1;").trim(), "x = y + 0x1;");
    }

    #[test]
    fn keeps_string_concatenation() {
        assert_eq!(run_rule(r#"x = "a" + "b";"#).trim(), r#"x = "a" + "b";"#);
    }

    #[test]
    fn folds_boolean_operand_via_coercion() {
        assert_eq!(run_rule("x = true & 0x1;").trim(), "x = 1;");
    }

    #[test]
    fn folds_null_operand_via_coercion() {
        assert_eq!(run_rule("x = null | 0x2;").trim(), "x = 2;");
    }

    #[test]
    fn folds_bigint_addition() {
        assert_eq!(run_rule("x = 0x1n + 0x2n;").trim(), "x = 3n;");
    }

    #[test]
    fn keeps_bigint_division_by_zero() {
        assert_eq!(run_rule("x = 0x6n / 0n;").trim(), "x = 0x6n / 0n;");
    }

    #[test]
    fn keeps_comparison_result() {
        assert_eq!(run_rule("x = 0x1 < 0x2;").trim(), "x = 0x1 < 0x2;");
    }
}
