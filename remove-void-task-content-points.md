# void-to-undefined scope-precision — raw facts for YOU to write the prompt

I did NOT write the task prompt. Write it yourself, in your own voice (the
AI-detection gate checks a human wrote it). Keep it 100-200 words, ASCII only.
Remember what worked last time: write it rough and natural, like explaining to a
teammate; do not polish it into clean formal prose.

## What the rule must do (behavior)

- Rewrite `void 0` (and `void <any number>`) to `undefined`.
- The catch: `void 0` is ALWAYS the real undefined, but the identifier
  `undefined` is whatever `undefined` means in that scope. They're only the same
  when `undefined` is not shadowed. So only convert where `undefined` still means
  the real global.
- The current rule is too blunt: if `undefined` is declared ANYWHERE in the file,
  it refuses to convert `void 0` anywhere. Make it work scope by scope instead.

## When to PRESERVE `void 0` (leave it alone)

- Inside any scope that binds its own `undefined`:
  - a function parameter named `undefined` (including destructured, e.g.
    `function f({ undefined })`), and the same for arrow parameters;
  - a `var undefined` / `function undefined` (these cover the whole function
    because of hoisting);
  - a block-scoped `let`/`const`/`class undefined` (shadows that block only;
    note it still applies to a `void 0` written *before* the `let`, because of
    the temporal dead zone);
  - a `for`-head binding, e.g. `for (let undefined of xs)`, `for (let undefined
    in o)`, `for (let undefined = 0; ...)`;
  - a named function expression whose own name is `undefined`
    (`const f = function undefined() { ... }` — the name is bound inside it);
  - a `catch (undefined)` (including destructured `catch ({ undefined })`).
- A module-level `undefined` binding (var/let/const/function/class/import)
  shadows the whole file.
- Inside a `with (...)` block (undefined could be a property of that object).
- Inside a function that runs direct `eval` (eval could define undefined).

## Scope precision (the important part)

- Shadowing in one scope must NOT block conversion in unrelated scopes. If one
  function shadows `undefined`, a sibling function's `void 0` still converts.
- Same for eval: eval in one function does not block conversions elsewhere.

## Constraints (closing lines)

- Harden the existing rule; keep its public constructor signature unchanged.
- Add regression tests for the convert cases and every preserve case.
- Preserve unrelated behavior; run the full suite, fmt, clippy.

## Harness facts (NOT part of the prompt)

- Baseline commit: d87c583a
- Existing test target (base): remove_void_rule
- Held-out test target (new): remove_void_scope_shadow_6f3a1c
- Public API used by held-out test: `RemoveVoid::new(unresolved_mark)`
- Note: the solution updates one existing test (`does_not_transform_when_undefined_is_declared`)
  because the old expectation was actually unsound; the prompt's scope-precision
  wording covers why.
