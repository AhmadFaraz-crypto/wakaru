mod common;

use common::{assert_eq_normalized, render};

/// The canonical Dean Edwards packer wrapper function expression. Only the
/// invocation arguments differ between inputs.
const BODY: &str = r"function(p,a,c,k,e,r){e=function(c){return(c<a?'':e(parseInt(c/a)))+((c=c%a)>35?String.fromCharCode(c+29):c.toString(36))};if(!''.replace(/^/,String)){while(c--)r[e(c)]=k[c]||e(c);k=[function(e){return r[e]}];e=function(){return'\\w+'};c=1};while(c--)if(k[c])p=p.replace(new RegExp('\\b'+e(c)+'\\b','g'),k[c]);return p}";

/// Build `eval(<wrapper>(payload, radix, count, dict.split('|'), 0, {}))`.
fn packer(payload: &str, radix: u32, count: u32, dict: &str) -> String {
    let payload = payload.replace('\\', "\\\\").replace('"', "\\\"");
    format!("eval({BODY}(\"{payload}\",{radix},{count},\"{dict}\".split('|'),0,{{}}))")
}

// --- decodes (packer is expanded back to source) ----------------------------

#[test]
fn decodes_function_declaration() {
    let input = packer("0 1(){2 3}", 62, 4, "function|foo|return|bar");
    assert_eq_normalized(&render(&input), "function foo() {\n    return bar;\n}");
}

#[test]
fn decoded_source_is_further_unminified() {
    // The recovered code feeds the rest of the pipeline: concat -> template,
    // function expression -> arrow, var -> const.
    let input = packer(
        "0 1=2(3){4 \"5 \"+3};6.7(1(\"8\"))",
        62,
        9,
        "var|greet|function|name|return|hi|console|log|world",
    );
    assert_eq_normalized(
        &render(&input),
        "const greet = (name)=>`hi ${name}`;\nconsole.log(greet(\"world\"));",
    );
}

#[test]
fn decodes_if_else_blocks() {
    let input = packer(
        "0(1){2(3)}4{5(3)}",
        62,
        6,
        "if|ready|start|engine|else|stop",
    );
    assert_eq_normalized(
        &render(&input),
        "if (ready) {\n    start(engine);\n} else {\n    stop(engine);\n}",
    );
}

#[test]
fn decodes_base36_radix() {
    let input = packer("0 1=2(3,4)", 36, 5, "var|sum|add|left|right");
    assert_eq_normalized(&render(&input), "const sum = add(left, right);");
}

#[test]
fn decodes_multiple_statements() {
    let input = packer(
        "0 1=2(3);0 4=5(3);6(1,4)",
        62,
        7,
        "const|first|head|list|rest|tail|join",
    );
    assert_eq_normalized(
        &render(&input),
        "const first = head(list);\nconst rest = tail(list);\njoin(first, rest);",
    );
}

#[test]
fn decodes_object_literal() {
    let input = packer("0 1={2:3,4:5}", 62, 6, "var|config|name|value|id|total");
    assert_eq_normalized(
        &render(&input),
        "const config = {\n    name: value,\n    id: total\n};",
    );
}

#[test]
fn decodes_for_of_loop() {
    let input = packer("0(1 2 3){4(1)}", 62, 5, "for|item|of|items|use");
    assert_eq_normalized(&render(&input), "for (item of items){\n    use(item);\n}");
}

#[test]
fn decodes_in_place_preserving_siblings() {
    let input = format!(
        "const header = 1;\n{};\nconst footer = 2;",
        packer("0 1(){2 3}", 62, 4, "function|foo|return|bar")
    );
    assert_eq_normalized(
        &render(&input),
        "const header = 1;\nfunction foo() {\n    return bar;\n}\nconst footer = 2;",
    );
}

#[test]
fn decodes_preserving_numeric_tokens() {
    let input = packer("0 1=2+3", 62, 4, "var|total|base|10");
    assert_eq_normalized(&render(&input), "const total = base + 10;");
}

#[test]
fn decodes_substitution_inside_string_literals() {
    // `done` is dictionary index 1; it must be restored inside the string too.
    let input = packer("0(\"1\")", 62, 2, "log|done");
    assert_eq_normalized(&render(&input), "log(\"done\");");
}

// --- guards (left untouched) -------------------------------------------------

#[test]
fn ignores_eval_of_non_string_argument() {
    // Not a packer call and not even a string: nothing to decode.
    let out = render("eval(userInput)");
    assert!(
        out.contains("eval("),
        "eval(userInput) must be left intact: {out}"
    );
}

#[test]
fn ignores_eval_of_non_packer_iife() {
    // An immediately-invoked function that is not the packer wrapper.
    let out = render(r#"eval(function(x){return x}("hi"))"#);
    assert!(
        out.contains("eval("),
        "a non-packer eval IIFE must not be decoded: {out}"
    );
}

#[test]
fn ignores_packer_with_non_literal_radix() {
    // The radix is a runtime variable, so the token encoding is unknowable and
    // no static decode is possible.
    let input = format!("eval({BODY}(\"0 1\",rx,2,\"a|b\".split('|'),0,{{}}))");
    let out = render(&input);
    assert!(
        out.contains(".replace("),
        "a dynamic-radix wrapper must be left intact: {out}"
    );
}

#[test]
fn ignores_packer_decoding_to_invalid_source() {
    // Payload `0 0 0` with dictionary `function` decodes to the unparseable
    // `function function function`; the rule must bail and leave the eval.
    let input = packer("0 0 0", 62, 1, "function");
    let out = render(&input);
    assert!(
        out.contains(".replace("),
        "an undecodable payload must leave the eval intact: {out}"
    );
    assert!(
        !out.contains("function function"),
        "invalid decoded source must not be spliced in: {out}"
    );
}

#[test]
fn ignores_packer_with_non_literal_payload() {
    // The payload is a variable, so there is no string to decode.
    let input = format!("eval({BODY}(src,62,2,\"a|b\".split('|'),0,{{}}))");
    let out = render(&input);
    assert!(
        out.contains("eval("),
        "a wrapper with a non-literal payload must be left intact: {out}"
    );
}

#[test]
fn ignores_packer_with_non_literal_count() {
    // The dictionary count is a variable, so the substitution range is unknown.
    let input = format!("eval({BODY}(\"0 1\",62,n,\"a|b\".split('|'),0,{{}}))");
    let out = render(&input);
    assert!(
        out.contains("eval("),
        "a wrapper with a non-literal count must be left intact: {out}"
    );
}

#[test]
fn ignores_packer_with_non_literal_dictionary() {
    // The dictionary source is a variable, so the word list is unknown.
    let input = format!("eval({BODY}(\"0 1\",62,2,words.split('|'),0,{{}}))");
    let out = render(&input);
    assert!(
        out.contains("eval("),
        "a wrapper with a non-literal dictionary must be left intact: {out}"
    );
}

#[test]
fn ignores_six_param_function_that_is_not_a_packer() {
    // Same parameter names and argument shape as the packer, but the body does
    // no substitution, so decoding it would produce the wrong program.
    let input = r#"eval(function(p,a,c,k,e,r){return p+a}("0 1",2,2,"foo|bar".split('|'),0,{}))"#;
    let out = render(input);
    assert!(
        out.contains("eval("),
        "a non-packer lookalike must not be decoded: {out}"
    );
}
