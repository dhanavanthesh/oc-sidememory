import json

import oc_sidememory

from _transformers import accepts, load_runtime, masked_next_token


model, tokenizer, vocabulary = load_runtime()
schema = json.dumps({
    "type": "object",
    "properties": {
        "id": {"type": "number"},
        "values": {
            "type": "array",
            "items": {"type": "string", "enum": ["a", "b"]},
            "minItems": 2,
            "maxItems": 2,
            "uniqueItems": True,
            "contains": {"const": "b"},
        },
        "confirmation": {"type": "number"},
    },
    "required": ["id", "values", "confirmation"],
    "additionalProperties": False,
})
extensions = json.dumps({
    "version": 1,
    "objects": [{
        "schemaPath": "$",
        "propertyOrder": ["id", "values", "confirmation"],
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
guide = oc_sidememory.SidememoryGuide(compiled, max_rollback=4)
valid = '{"id":1,"values":["a","b"],"confirmation":1.0}'
invalid = '{"id":1,"values":["a","a"],"confirmation":1}'
for token in tokenizer.encode(valid, add_special_tokens=False):
    guide.advance(token)
assert guide.get_allowed_rollback() > 0
guide.rollback(guide.get_allowed_rollback())
guide.reset()
assert accepts(guide, tokenizer, valid)
assert not accepts(guide, tokenizer, invalid)
selected = masked_next_token(model, tokenizer, guide)
print(f"composition: real-model token {selected!r}; rollback and rejection verified")
