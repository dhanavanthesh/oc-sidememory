# Supported JSON Schema profile

OC-Sidememory compiles a checked subset of JSON Schema Draft 2020-12.
Unsupported assertions fail with structured diagnostics.

## Serialization policy

- Numeric `const` and `enum` literals use plain decimal when that spelling
  is at most 4096 digits. Larger expansions use exact scientific notation.
- Scientific notation emitted through `serde_json` may include a `+` before
  a positive exponent.
- String literals match their exact JSON encoding. Equivalent `\uXXXX`
  spellings are not generated as alternatives.
- Structural whitespace uses the legacy policy of one optional space.

## Supported assertions

| Feature | Status |
|---|---|
| Boolean schema | Supported |
| Explicit scalar type | Supported |
| Homogeneous typed array and `items` | Supported within the profile |
| `minItems` and `maxItems` | Supported |
| `uniqueItems: false` | Supported as a no-op |
| `uniqueItems: true` | Compiled into a `MemoryPlan`; runtime enforcement is not available |
| Explicit object `properties` and `required` | Supported |
| `additionalProperties: false` | Supported |
| Exact scalar `const` and `enum` | Supported |
| Composite `const` and `enum` | Unsupported |

Known annotations may be accepted according to `CheckedProfile` without
assertion behavior.

## Unsupported assertions

`allOf`, `anyOf`, `oneOf`, `not`, `$ref`, `prefixItems`, `contains`,
conditionals, dependent assertions, unevaluated assertions, pattern
properties, and format assertions are rejected explicitly. Unknown extension
keywords follow the configured profile policy and are never silently treated
as assertions.
