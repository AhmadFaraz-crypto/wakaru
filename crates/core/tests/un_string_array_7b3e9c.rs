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

// --- inlines -----------------------------------------------------------------

#[test]
fn inlines_computed_member_key_and_argument() {
    assert_eq_normalized(
        &render(r#"const s = ["log", "hello"]; console[s[0]](s[1]);"#),
        r#"console.log("hello");"#,
    );
}

#[test]
fn inlines_multiple_reads_and_drops_array() {
    assert_eq_normalized(
        &render(r#"const s = ["a", "b", "c"]; foo(s[2], s[0]);"#),
        r#"foo("c", "a");"#,
    );
}

#[test]
fn inlines_var_declared_array() {
    assert_eq_normalized(&render(r#"var s = ["x"]; bar(s[0]);"#), r#"bar("x");"#);
}

#[test]
fn keeps_array_for_unresolved_index() {
    assert_eq_normalized(
        &render(r#"const s = ["x", "y"]; bar(s[0], s[i]);"#),
        "const s = [\n    \"x\",\n    \"y\"\n];\nbar(\"x\", s[i]);",
    );
}

#[test]
fn only_folds_the_top_level_array() {
    // The inner `s` is a different binding and must be untouched.
    let out = render(
        r#"const s = ["outer"]; function f() { const s = ["inner"]; return s[0]; } g(s[0]);"#,
    );
    assert!(
        out.contains(r#"g("outer")"#),
        "outer read should fold: {out}"
    );
    assert!(out.contains(r#""inner""#), "inner array must stay: {out}");
    assert!(out.contains("return s[0]"), "inner read must stay: {out}");
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_out_of_range_index() {
    let out = render(r#"const s = ["x"]; bar(s[5]);"#);
    assert!(out.contains("s[5]"), "out-of-range read must stay: {out}");
}

#[test]
fn ignores_reassigned_array() {
    let out = render(r#"const s = ["x"]; s = y; bar(s[0]);"#);
    assert!(
        out.contains("s[0]"),
        "a reassigned array must not fold: {out}"
    );
}

#[test]
fn ignores_element_write() {
    let out = render(r#"const s = ["x", "y"]; s[0] = "z"; bar(s[0]);"#);
    assert!(
        out.contains("s[0]"),
        "an element-written array must not fold: {out}"
    );
}

#[test]
fn ignores_mutating_method_call() {
    let out = render(r#"const s = ["x", "y"]; s.push("z"); bar(s[0]);"#);
    assert!(
        out.contains("s[0]"),
        "a method-mutated array must not fold: {out}"
    );
}

#[test]
fn ignores_static_member_access() {
    let out = render(r#"const s = ["x", "y"]; bar(s.length, s[0]);"#);
    assert!(
        out.contains("s[0]"),
        "a statically-accessed array must not fold: {out}"
    );
}

#[test]
fn ignores_array_passed_by_reference() {
    let out = render(r#"const s = ["x", "y"]; use(s); bar(s[0]);"#);
    assert!(out.contains("s[0]"), "a passed array must not fold: {out}");
}

#[test]
fn ignores_non_string_element() {
    let out = render(r#"const s = ["x", 1]; bar(s[0]);"#);
    assert!(
        out.contains("s[0]"),
        "a mixed array is not a candidate: {out}"
    );
}
