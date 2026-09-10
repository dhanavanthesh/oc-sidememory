# Supported schema profile

OC-Sidememory accepts a checked subset of JSON Schema Draft 2020-12. Unsupported assertions produce compile diagnostics.

| Feature | Support |
|---|---|
| Boolean schemas and scalar `type` | Yes |
| Exact scalar `const` and `enum` | Yes |
| Composite `const` and `enum` | No |
| Homogeneous array `items`, `minItems`, `maxItems` | Yes |
| `uniqueItems` | Exact generation-time enforcement |
| `contains`, `minContains`, `maxContains` | Yes, with the predicate subset below |
| Closed `properties` and `required` objects | Yes |

`minContains` defaults to 1. A value of 0 is valid. Bounds without adjacent `contains` have no effect.

Contains predicates support booleans, checked scalar types, exact scalar constants and enums, closed objects, `required`, homogeneous arrays, item bounds, and nested `uniqueItems`. Nested `contains`, references, combinators, conditionals, unevaluated assertions, and unsupported composite constants are rejected.

The general compiler does not support `$ref`, `allOf`, `anyOf`, `oneOf`, `not`, `prefixItems`, conditionals, dependent assertions, unevaluated assertions, pattern properties, or format assertions.

The version 1 extension supports fixed-order direct-property captures, equality, inequality, membership, and non-membership. Forward references, cycles, unknown names, incompatible types, and ambiguous scopes fail compilation.

Numbers are compared exactly without floating-point conversion. String equality uses decoded code points without Unicode normalization. Object equality ignores property order; array equality preserves it.
