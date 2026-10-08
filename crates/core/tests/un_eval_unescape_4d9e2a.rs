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

// --- decodes -----------------------------------------------------------------

#[test]
fn decodes_percent_hex() {
    assert_eq_normalized(&render(r#"eval(unescape("%66%6f%6f"));"#), "foo;");
}

#[test]
fn decodes_mixed_literal_and_escapes() {
    assert_eq_normalized(
        &render(r#"eval(unescape("alert(%22hi%22)"));"#),
        "alert(\"hi\");",
    );
}

#[test]
fn decodes_function_body() {
    assert_eq_normalized(
        &render(r#"eval(unescape("function run(){return %2242%22}"));"#),
        "function run() {\n    return \"42\";\n}",
    );
}

#[test]
fn decodes_percent_u_escapes() {
    assert_eq_normalized(&render(r#"eval(unescape("%u0066oo"));"#), "foo;");
}

#[test]
fn decodes_in_place_preserving_siblings() {
    assert_eq_normalized(
        &render(r#"const a = 1; eval(unescape("%66%6f%6f")); const b = 2;"#),
        "const a = 1;\nfoo;\nconst b = 2;",
    );
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_undecodable_source() {
    // Decodes to "function {", which is not valid JavaScript: bail, don't crash.
    let out = render(r#"eval(unescape("function %7b"));"#);
    assert!(
        out.contains("unescape"),
        "an undecodable payload must be left: {out}"
    );
}

#[test]
fn ignores_non_string_argument() {
    let out = render(r#"eval(unescape(x));"#);
    assert!(
        out.contains("unescape"),
        "a non-literal argument must be left: {out}"
    );
}

#[test]
fn ignores_eval_of_plain_string() {
    let out = render(r#"eval("alert(1)");"#);
    assert!(
        out.contains("eval("),
        "a non-unescape eval must be left: {out}"
    );
}

#[test]
fn ignores_unescape_without_eval() {
    let out = render(r#"var s = unescape("%66%6f%6f");"#);
    assert!(
        out.contains("unescape"),
        "unescape outside eval must be left: {out}"
    );
}

#[test]
fn ignores_shadowed_local_unescape() {
    let out = render(r#"function decode(unescape) { return eval(unescape("%66%6f%6f")); }"#);
    assert!(
        out.contains("unescape"),
        "a shadowed local unescape must be left: {out}"
    );
}

#[test]
fn ignores_eval_with_extra_arguments() {
    let out = render(r#"eval(unescape("%66%6f%6f"), 0);"#);
    assert!(
        out.contains("unescape"),
        "eval with extra arguments must be left: {out}"
    );
}
