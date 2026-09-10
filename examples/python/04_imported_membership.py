import json

import oc_sidememory

from _transformers import accepts, load_runtime, masked_next_token


model, tokenizer, vocabulary = load_runtime()
schema = json.dumps({
    "type": "object",
    "properties": {"customer": {"type": "string"}},
    "required": ["customer"],
    "additionalProperties": False,
})
extensions = json.dumps({
    "version": 1,
    "objects": [{
        "schemaPath": "$",
        "propertyOrder": ["customer"],
        "relations": [{
            "targetProperty": "customer",
            "operator": "memberOf",
            "import": "valid_customers",
        }],
    }],
})
compiled = oc_sidememory.compile_schema(
    schema, vocabulary, tokenizer.vocab_size, extensions_json=extensions
)
imports = oc_sidememory.ImportedMemory.from_json(
    "customer-catalog", "1", '{"valid_customers":["cust_7","cust_9"]}'
)
guide = oc_sidememory.SidememoryGuide(compiled, imports=imports)
assert accepts(guide, tokenizer, '{"customer":"cust_7"}')
assert not accepts(guide, tokenizer, '{"customer":"cust_8"}')
selected = masked_next_token(model, tokenizer, guide)
print(f"membership: real-model token {selected!r}; non-member rejected")
