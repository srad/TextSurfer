# CSS custom properties and `var()`

Status: done (2026-08-25)

## Goal

Implement the stable CSS Custom Properties Level 1 behavior that affects rendering without adding
custom properties to `ComputedStyle` or `StyleTree`.

## Contract

- cssparser 0.37.0 owns tokenization, blocks, escapes, source positions and malformed-input
  rejection. Ordinary property names are ASCII-lowercased; decoded custom names preserve case and
  code points, and the reserved `--` name is rejected.
- Custom values preserve their source spelling after CSS-whitespace trimming and valid trailing
  `!important` removal. Empty values are valid. Bad strings/URLs, unmatched blocks and top-level
  `!` are invalid declarations.
- `var()` is ASCII-case-insensitive. Its first argument is one direct custom-property name; the
  first top-level comma begins a possibly empty fallback, whose further commas are literal.
- The cascade selects custom declarations by the existing importance/specificity/order sort, then
  computes an inherited, case-sensitive environment for each element. Local values are resolved
  before inheritance, references in fallbacks participate in cycle detection, and empty, missing,
  invalid and CSS-wide values remain distinct.
- Substitution traverses component values rather than strings, never substitutes inside quoted
  strings or comments, and preserves token boundaries. Values stay as tokens until a supported
  property consumes the flattened result.
- Every supported property consumer receives substituted declarations, including `font-size`,
  style/flex properties, counters and generated content. A variable-bearing declaration that fails
  substitution or the post-substitution grammar computes to `unset` and hides a lower declaration;
  ordinary invalid declarations keep their existing lower-wins behavior. Shorthand invalidation is
  atomic. `revert` restores the captured UA baseline; `revert-layer` is unsupported.
- Computed custom values and substituted declarations are limited to 2 MiB and component nesting to
  64. Dependency and environment lookup do not recurse on the Rust call stack. Tests inject smaller
  limits for hostile-chain and expansion cases.

## Order

1. Pin parser and resolver contracts with failing tests.
2. Add `css::variables` syntax, environment and substitution modules.
3. Integrate element and pseudo environments into the cascade, then route all declaration consumers
   through the resolved value.
4. Add public render fixture and style/geometry equivalence assertions.
5. Update README and ROADMAP evidence, run the complete feature gate matrix, and compare the selector
   benchmark against the 6.487 ms baseline. A confirmed median regression over 15% blocks closure.

## Exclusions

CSSOM, animation taint, `@property`, `env()`, Custom Properties Level 2 dynamic names and variable
units, cascade layers/`revert-layer`, `@supports`, and CSS math functions. Variables may carry math
tokens, but property parsing rejects them until the separate CSS math item lands.
