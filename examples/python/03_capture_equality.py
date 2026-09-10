import json

import oc_sidememory

from _transformers import accepts, load_runtime, masked_next_token


model, tokenizer, vocabulary = load_runtime()
schema = json.dumps({
    "type": "object",
    "properties": {"id": {"type": "number"}, "confirmation": {"type": "number"}},
    "required": ["id", "confirmation"],
    "additionalProperties": False,
})
extensions = json.dumps({
    "version": 1,
    "objects": [{
        "schemaPath": "$",
        "propertyOrder": ["id", "confirmation"],
        "captures": [{"name": "primary", "sourceProperty": "id"}],
        "relations": [{
            "targetProperty": "confirmation",
            "operator": "equal",
            "capture": "primary",
        }],
    }],
})
compiled = oc_sidememory.compile_schema(
    schema, vocabulary, tokenizer.vocab_size, extensions_json=extensions
)
guide = oc_sidememory.SidememoryGuide(compiled)
assert accepts(guide, tokenizer, '{"id":1,"confirmation":1.0}')
assert not accepts(guide, tokenizer, '{"id":1,"confirmation":2}')
selected = masked_next_token(model, tokenizer, guide)
print(f"capture equality: real-model token {selected!r}; mismatch rejected")
