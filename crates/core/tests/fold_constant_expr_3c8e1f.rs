mod common;

use common::assert_eq_normalized;
use wakaru_core::{decompile, DecompileOptions};

fn render(source: &str) -> String {
    decompile(
        source,
        DecompileOptions {
            filename: "fixture.js".to_string(),
            ..Default::default()
        },
    )
    .expect("decompile should succeed")
    .code
}

// --- folds the obfuscated constant shape ------------------------------------

#[test]
fn folds_hex_addition() {
    assert_eq_normalized(&render("var x = 0x1a + 0x5;"), "const x = 31;");
}

#[test]
fn folds_bitwise_xor() {
    assert_eq_normalized(&render("var x = 0x45 ^ 0x4e;"), "const x = 11;");
}

#[test]
fn folds_bitwise_not_to_negative() {
    assert_eq_normalized(&render("var x = ~0xff;"), "const x = -256;");
}

#[test]
fn folds_left_shift_with_decimal_operands() {
    assert_eq_normalized(&render("var x = 1 << 4;"), "const x = 16;");
}

#[test]
fn folds_unsigned_right_shift_to_u32_range() {
    assert_eq_normalized(&render("var x = -1 >>> 0;"), "const x = 4294967295;");
}

#[test]
fn folds_bitwise_or_of_hex_operands() {
    assert_eq_normalized(&render("var x = 0xf0 | 0x0f;"), "const x = 255;");
}

#[test]
fn folds_nested_parenthesized_subtree() {
    assert_eq_normalized(&render("var x = (0x2 + 0x3) << 1;"), "const x = 10;");
}

#[test]
fn folds_inside_a_member_index() {
    assert_eq_normalized(
        &render("function f(a) { return a[0x2 + 0x1]; }"),
        "function f(a) { return a[3]; }",
    );
}

// --- leaves everything else as written --------------------------------------

#[test]
fn keeps_plain_decimal_arithmetic() {
    // No bitwise operator and no non-decimal literal: the factored form is the
    // more readable one, so it is left alone.
    assert_eq_normalized(&render("var x = 60 * 1000;"), "const x = 60 * 1000;");
}

#[test]
fn keeps_decimal_exponent() {
    assert_eq_normalized(&render("var x = 2 ** 10;"), "const x = 2 ** 10;");
}

#[test]
fn keeps_marker_op_with_non_literal_operand() {
    // The shift marker is present, but an identifier operand makes the
    // expression non-constant, so nothing folds.
    assert_eq_normalized(&render("var x = y << 2;"), "const x = y << 2;");
}

#[test]
fn keeps_non_finite_division() {
    // Division by zero evaluates to Infinity, which has no clean literal
    // spelling, so the expression is preserved (the hex literal is only
    // respelled in decimal by the later numeric-literal pass).
    assert_eq_normalized(&render("var x = 0xff / 0;"), "const x = 255 / 0;");
}

#[test]
fn folds_boolean_operand_via_coercion() {
    // ECMAScript coerces the boolean to a number, so `true & 1` is `1`.
    assert_eq_normalized(&render("var x = true & 0x1;"), "const x = 1;");
}

#[test]
fn folds_bigint_addition() {
    // An all-BigInt subtree folds to a BigInt literal.
    assert_eq_normalized(&render("var x = 0x1n + 0x2n;"), "const x = 3n;");
}

#[test]
fn keeps_bigint_division_by_zero() {
    // BigInt division by zero throws in ECMAScript, so it is left as written.
    assert_eq_normalized(&render("var x = 0x6n / 0n;"), "const x = 0x6n / 0n;");
}

#[test]
fn keeps_comparison_result() {
    // A comparison evaluates to a boolean, not a number: not a numeric fold.
    assert_eq_normalized(&render("var x = 0x1 < 0x2;"), "const x = 1 < 2;");
}
