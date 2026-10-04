mod common;

use common::{assert_eq_normalized, render};

// --- decodes -----------------------------------------------------------------

#[test]
fn decodes_eval_atob_call() {
    assert_eq_normalized(&render(r#"eval(atob("YWxlcnQoMSk="))"#), "alert(1);");
}

#[test]
fn decodes_eval_atob_function() {
    assert_eq_normalized(
        &render(r#"eval(atob("ZnVuY3Rpb24gZm9vKCl7cmV0dXJuIGJhcn0="))"#),
        "function foo() {\n    return bar;\n}",
    );
}

#[test]
fn decoded_source_is_further_unminified() {
    // var -> const, and the recovered code flows through the rest of the pipeline.
    assert_eq_normalized(
        &render(r#"eval(atob("dmFyIGdyZWV0PSJoaSI7Y29uc29sZS5sb2coZ3JlZXQp"))"#),
        "const greet = \"hi\";\nconsole.log(greet);",
    );
}

#[test]
fn decodes_in_place_preserving_siblings() {
    assert_eq_normalized(
        &render(r#"const a = 1; eval(atob("YWxlcnQoMSk=")); const b = 2;"#),
        "const a = 1;\nalert(1);\nconst b = 2;",
    );
}

#[test]
fn decodes_base64_with_whitespace() {
    // atob ignores ASCII whitespace inside the base64.
    assert_eq_normalized(&render(r#"eval(atob("YW xl cnQo MSk="))"#), "alert(1);");
}

#[test]
fn decodes_base64_without_padding() {
    // atob tolerates missing "=" padding.
    assert_eq_normalized(&render(r#"eval(atob("aGk"))"#), "hi;");
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_eval_with_extra_arguments() {
    // The second eval argument is evaluated and cannot be silently dropped.
    let out = render(r#"eval(atob("YWxlcnQoMSk="), 0)"#);
    assert!(
        out.contains("atob"),
        "eval with extra arguments must be left: {out}"
    );
}

#[test]
fn ignores_invalid_base64() {
    let out = render("eval(atob(\"###\"))");
    assert!(out.contains("atob"), "invalid base64 must be left: {out}");
}

#[test]
fn ignores_non_string_argument() {
    let out = render(r#"eval(atob(x))"#);
    assert!(
        out.contains("atob"),
        "a non-literal atob argument must be left: {out}"
    );
}

#[test]
fn ignores_eval_of_plain_string() {
    // `eval` of a plain string, not `atob(...)`.
    let out = render(r#"eval("alert(1)")"#);
    assert!(out.contains("eval("), "a non-atob eval must be left: {out}");
}

#[test]
fn ignores_atob_without_eval() {
    let out = render(r#"var s = atob("YWxlcnQoMSk=");"#);
    assert!(
        out.contains("atob"),
        "atob outside eval must be left: {out}"
    );
}

#[test]
fn ignores_undecodable_source() {
    // Decodes to "function {{{", which is not valid JavaScript: bail, don't crash.
    let out = render(r#"eval(atob("ZnVuY3Rpb24ge3t7"))"#);
    assert!(
        out.contains("atob"),
        "an undecodable payload must be left: {out}"
    );
}

#[test]
fn ignores_shadowed_local_atob() {
    // `atob` here is a parameter, not the global.
    let out = render(r#"function decode(atob) { return eval(atob("YWxlcnQoMSk=")); }"#);
    assert!(
        out.contains("atob"),
        "a shadowed local atob must be left: {out}"
    );
}

#[test]
fn ignores_atob_with_extra_arguments() {
    let out = render(r#"eval(atob("YWxlcnQoMSk=", 2))"#);
    assert!(
        out.contains("atob"),
        "atob with extra arguments must be left: {out}"
    );
}
