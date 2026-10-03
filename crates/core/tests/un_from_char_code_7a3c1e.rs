mod common;

use common::{assert_eq_normalized, render};

// --- folds -------------------------------------------------------------------

#[test]
fn folds_ascii_character_codes() {
    assert_eq_normalized(
        &render("const x = String.fromCharCode(72, 101, 108, 108, 111);"),
        "const x = \"Hello\";",
    );
}

#[test]
fn folds_empty_call_to_empty_string() {
    assert_eq_normalized(
        &render("const x = String.fromCharCode();"),
        "const x = \"\";",
    );
}

#[test]
fn folds_basic_multilingual_plane_code_unit() {
    assert_eq_normalized(
        &render("const x = String.fromCharCode(0x263a);"),
        "const x = \"\u{263a}\";",
    );
}

#[test]
fn folds_surrogate_pair_to_astral_character() {
    assert_eq_normalized(
        &render("const x = String.fromCharCode(0xd83d, 0xde00);"),
        "const x = \"\u{1f600}\";",
    );
}

#[test]
fn folds_computed_member_key_then_simplifies_access() {
    // obj[String.fromCharCode(107,101,121)] -> obj["key"] -> obj.key
    let out = render("const v = obj[String.fromCharCode(107, 101, 121)];");
    assert!(out.contains("obj.key"), "expected obj.key, got: {out}");
}

#[test]
fn folds_each_call_independently() {
    assert_eq_normalized(
        &render("const s = String.fromCharCode(72) + String.fromCharCode(105);"),
        "const s = \"H\" + \"i\";",
    );
}

#[test]
fn folds_call_argument() {
    assert_eq_normalized(
        &render("log(String.fromCharCode(104, 105));"),
        "log(\"hi\");",
    );
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_non_literal_argument() {
    let out = render("const x = String.fromCharCode(code);");
    assert!(
        out.contains("fromCharCode"),
        "must not fold a variable: {out}"
    );
}

#[test]
fn ignores_lone_surrogate() {
    let out = render("const x = String.fromCharCode(0xd83d);");
    assert!(
        out.contains("fromCharCode"),
        "a lone surrogate has no UTF-8 form and must be left: {out}"
    );
}

#[test]
fn ignores_out_of_range_value() {
    let out = render("const x = String.fromCharCode(0x10000);");
    assert!(
        out.contains("fromCharCode"),
        "a value above 0xFFFF must be left (no ToUint16 emulation): {out}"
    );
}

#[test]
fn ignores_negative_value() {
    let out = render("const x = String.fromCharCode(-1);");
    assert!(
        out.contains("fromCharCode"),
        "a negative value must be left: {out}"
    );
}

#[test]
fn ignores_fractional_value() {
    let out = render("const x = String.fromCharCode(65.9);");
    assert!(
        out.contains("fromCharCode"),
        "a fractional value must be left: {out}"
    );
}

#[test]
fn ignores_from_code_point() {
    // A different static method with different semantics.
    let out = render("const x = String.fromCodePoint(65);");
    assert!(
        out.contains("fromCodePoint"),
        "fromCodePoint must be left: {out}"
    );
}

#[test]
fn ignores_spread_arguments() {
    let out = render("const x = String.fromCharCode(...codes);");
    assert!(
        out.contains("fromCharCode"),
        "spread args must be left: {out}"
    );
}

#[test]
fn ignores_shadowed_local_string() {
    // `String` is a local parameter, not the global constructor.
    let out = render("function f(String) {\n    return String.fromCharCode(72);\n}");
    assert!(
        out.contains("fromCharCode"),
        "a shadowed local String must not be folded: {out}"
    );
}

#[test]
fn ignores_partially_non_literal_arguments() {
    // One argument is a variable, so the whole call is unknowable.
    let out = render("const x = String.fromCharCode(72, code);");
    assert!(
        out.contains("fromCharCode"),
        "a mix of literal and non-literal args must be left: {out}"
    );
}

#[test]
fn ignores_from_char_code_on_other_receiver() {
    // `fromCharCode` on something that is not the global String.
    let out = render("const x = obj.fromCharCode(72);");
    assert!(
        out.contains("fromCharCode"),
        "fromCharCode on a non-String receiver must be left: {out}"
    );
}

#[test]
fn ignores_reversed_surrogate_pair() {
    // Low surrogate before high is not a valid pair: invalid UTF-16.
    let out = render("const x = String.fromCharCode(0xde00, 0xd83d);");
    assert!(
        out.contains("fromCharCode"),
        "a reversed surrogate pair must be left: {out}"
    );
}

#[test]
fn ignores_unpaired_high_surrogate() {
    // A high surrogate followed by a non-surrogate is invalid UTF-16.
    let out = render("const x = String.fromCharCode(0xd83d, 0x41);");
    assert!(
        out.contains("fromCharCode"),
        "an unpaired high surrogate must be left: {out}"
    );
}
