# Extension plan version 1

Cross-field relations are an OC-Sidememory generation extension, not standard JSON Schema. Each object entry declares `schemaPath`, `propertyOrder`, captures, and relations.

A completed source property can publish a capture. A later target can require equality, inequality, imported membership, or imported non-membership. Only direct properties in the same runtime object are supported. Relation checks run before captures for the same event.

Unknown or duplicate names, forward references, cycles, incompatible types, ambiguous scopes, and unsupported versions fail compilation.
