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

// --- recovers export * -------------------------------------------------------

#[test]
fn recovers_bare_export_star() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("./mod"), exports);"#),
        "export * from \"./mod\";",
    );
}

#[test]
fn recovers_namespaced_export_star() {
    assert_eq_normalized(
        &render(
            "var tslib_1 = require(\"tslib\");\ntslib_1.__exportStar(require(\"./mod\"), exports);",
        ),
        "import tslib_1 from \"tslib\";\nexport * from \"./mod\";",
    );
}

#[test]
fn recovers_alongside_other_exports() {
    assert_eq_normalized(
        &render("__exportStar(require(\"./a\"), exports);\nconst b = require(\"./b\");\nexports.x = b.y;"),
        "import b from \"./b\";\nexport * from \"./a\";\nexport const x = b.y;",
    );
}

#[test]
fn recovers_multiple_re_exports() {
    assert_eq_normalized(
        &render(
            "__exportStar(require(\"./a\"), exports);\n__exportStar(require(\"./b\"), exports);",
        ),
        "export * from \"./a\";\nexport * from \"./b\";",
    );
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_non_require_first_argument() {
    let out = render(r#"__exportStar(something, exports);"#);
    assert!(
        out.contains("__exportStar"),
        "non-require arg must be left: {out}"
    );
}

#[test]
fn ignores_non_exports_second_argument() {
    let out = render(r#"__exportStar(require("./mod"), notExports);"#);
    assert!(
        out.contains("__exportStar"),
        "non-exports arg must be left: {out}"
    );
}

#[test]
fn ignores_other_method_call() {
    let out = render(r#"foo.bar(require("./mod"), exports);"#);
    assert!(
        out.contains("foo.bar"),
        "a different method must be left: {out}"
    );
}

#[test]
fn ignores_wrong_argument_count() {
    let out = render(r#"__exportStar(require("./mod"));"#);
    assert!(
        out.contains("__exportStar"),
        "a one-arg call must be left: {out}"
    );
}

#[test]
fn recovers_nested_namespace_export_star() {
    assert_eq_normalized(
        &render(r#"runtime.helpers.__exportStar(require("./mod"), exports);"#),
        r#"export * from "./mod";"#,
    );
}

#[test]
fn recovers_deep_namespace_export_star() {
    assert_eq_normalized(
        &render(r#"runtime.helpers.esm.__exportStar(require("./mod"), exports);"#),
        r#"export * from "./mod";"#,
    );
}

#[test]
fn recovers_computed_namespace_export_star() {
    assert_eq_normalized(
        &render(r#"runtime["helpers"].__exportStar(require("./mod"), exports);"#),
        r#"export * from "./mod";"#,
    );
}

#[test]
fn recovers_nested_computed_namespace_export_star() {
    assert_eq_normalized(
        &render(r#"runtime.helpers["tslib"].__exportStar(require("./mod"), exports);"#),
        r#"export * from "./mod";"#,
    );
}
