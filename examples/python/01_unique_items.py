import json

import oc_sidememory

from _transformers import accepts, load_runtime, masked_next_token


model, tokenizer, vocabulary = load_runtime()
schema = json.dumps({
    "type": "array",
    "items": {"type": "string", "enum": ["a", "b"]},
    "minItems": 2,
    "maxItems": 2,
    "uniqueItems": True,
})
guide = oc_sidememory.SidememoryGuide(
    oc_sidememory.compile_schema(schema, vocabulary, tokenizer.vocab_size)
)
assert accepts(guide, tokenizer, '["a","b"]')
assert not accepts(guide, tokenizer, '["a","a"]')
selected = masked_next_token(model, tokenizer, guide)
print(f"uniqueItems: real-model token {selected!r}; duplicate rejected")
