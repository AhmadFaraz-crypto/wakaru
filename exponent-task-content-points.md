# Exponent (Math.pow) hardening — raw facts for YOU to write the prompt

I did NOT write the task prompt/description. Turn these bullets into your own
prose (the AI-detection gate checks that a human authored the submitted text).
This is the HARDER version.

There is a function in JavaScript library Math.pow you need to find first this one and then you have to convert a, b because this is our problem part 
once you computed the a.b then you have to check globalThis and then const and Math aliases. after that we have chains you need to check. you need to make sure 
few things should never be changed `window.Math.pow` or `self.Math.pow` — only `globalThis` and please make sure Math formulas work according to that and please
treat every fnction in their own scope

## What the rule must do (behavior)

- Convert `Math.pow(a, b)` into `a ** b`.
- Only convert when `Math.pow` is provably the real built-in. Recognise these
  equivalent spellings of the built-in reference:
  - `Math.pow(a, b)` and the computed form `Math["pow"](a, b)`
  - `globalThis.Math.pow(a, b)` (and `globalThis.Math["pow"](a, b)`)
  - a stable `const` alias of `Math`, following alias chains:
    `const M = Math; const N = M; N.pow(a, b)`, and `const M = globalThis.Math`
  - a detached `const` alias of the method: `const p = Math.pow; p(a, b)`,
    `const { pow } = Math`, `const { pow: p } = M`, and `const q = p`
  - why the detached form is safe: `Math.pow` takes both operands as arguments
    and ignores its `this`, so calling it through an alias is the same call.
- Only stable `const` aliases qualify (a `let` alias could be reassigned).

## What it must leave unchanged (safety)

- Do not convert `window.Math.pow` or `self.Math.pow` — only `globalThis` is a
  guaranteed cross-environment name for the global object.
- Do not convert a computed member whose key is not the string `"pow"`
  (e.g. `Math[k]`).
- If the `Math.pow` slot is reassigned anywhere in the module, convert nothing:
  `Math = ...`, `Math.pow = ...`, `Math["pow"] = ...`, `globalThis.Math = ...`,
  a write through an alias `M.pow = ...`, or `Math++`.
- If `globalThis` itself is reassigned, stop converting the `globalThis.Math`
  spelling — but plain `Math.pow` is still the built-in and still converts.
- Do not convert when `Math` is lexically shadowed or a local binding.
- Do not convert inside a `with` block (there `Math` may resolve to a property
  of the with-object).
- Treat every function-like scope as its own `eval` boundary: functions,
  arrows, constructors, getters, setters, and class field / private-field /
  auto-accessor initializers. A direct `eval` in one scope blocks conversions
  only within that scope; `eval` elsewhere must not block unrelated conversions.
- Keep the existing argument checks: exactly two arguments, no spread; other
  `Math` methods and non-`Math` `.pow` calls untouched; complex/unary operands
  and nested powers stay correctly parenthesized.

## Constraints (for the prompt's closing lines)

- Harden the existing rule; keep its public constructor signature unchanged.
- Add focused regression tests for every case above.
- Preserve all unrelated behavior; run the full suite, fmt, and clippy.

## Harness facts (NOT part of the prompt)

- Baseline commit: d87c583a
- Existing test target (base): exponent_rule
- Held-out test target (new): exponent_binding_aware_4f2a7c
- Public API used by the held-out test: `Exponent::new(unresolved_mark)`
- Patches: exponent-test.patch, exponent-solution.patch (also copied to the
  active test.patch / solution.patch) — regenerated after the expansion
