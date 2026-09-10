import json

import oc_sidememory

from _transformers import accepts, load_runtime, masked_next_token


model, tokenizer, vocabulary = load_runtime()
schema = json.dumps({
    "type": "array",
    "items": {"type": "integer", "enum": [1, 2]},
    "minItems": 2,
    "maxItems": 2,
    "contains": {"const": 1},
    "minContains": 1,
    "maxContains": 1,
})
guide = oc_sidememory.SidememoryGuide(
    oc_sidememory.compile_schema(schema, vocabulary, tokenizer.vocab_size)
)
assert accepts(guide, tokenizer, "[1,2]")
assert not accepts(guide, tokenizer, "[2,2]")
selected = masked_next_token(model, tokenizer, guide)
print(f"contains: real-model token {selected!r}; missing match rejected")
