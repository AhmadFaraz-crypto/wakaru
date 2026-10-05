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

#[test]
fn recovers_bare_call() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("./mod"), exports);"#),
        "export * from \"./mod\";",
    );
}

#[test]
fn recovers_member_call_without_declaration() {
    // Any object as the receiver; only the `.__exportStar` method name matters.
    assert_eq_normalized(
        &render(r#"x.__exportStar(require("./mod"), exports);"#),
        "export * from \"./mod\";",
    );
}

#[test]
fn recovers_deeper_relative_path() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("../pkg/sub"), exports);"#),
        "export * from \"../pkg/sub\";",
    );
}

#[test]
fn recovers_bare_specifier() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("lodash"), exports);"#),
        "export * from \"lodash\";",
    );
}

#[test]
fn ignores_non_require_argument() {
    assert_eq_normalized(
        &render(r#"__exportStar(something, exports);"#),
        "__exportStar(something, exports);",
    );
}

#[test]
fn ignores_non_exports_target() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("./mod"), other);"#),
        "__exportStar(require(\"./mod\"), other);",
    );
}

#[test]
fn ignores_different_method() {
    assert_eq_normalized(
        &render(r#"obj.copy(require("./mod"), exports);"#),
        "obj.copy(require(\"./mod\"), exports);",
    );
}

#[test]
fn ignores_missing_exports_argument() {
    assert_eq_normalized(
        &render(r#"__exportStar(require("./mod"));"#),
        "__exportStar(require(\"./mod\"));",
    );
}
