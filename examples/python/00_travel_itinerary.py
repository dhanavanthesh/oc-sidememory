"""Generate a schema-constrained travel itinerary with Qwen."""

import json

import oc_sidememory
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer


MODEL_ID = "Qwen/Qwen2.5-0.5B-Instruct"
REVISION = "7ae557604adf67be50417f59c2c2f167def9a775"

SCHEMA = {
    "type": "object",
    "properties": {
        "city": {"type": "string", "enum": ["Kyoto", "Paris", "Reykjavik"]},
        "pace": {"type": "string", "enum": ["relaxed", "balanced", "busy"]},
        "activities": {
            "type": "array",
            "items": {
                "type": "string",
                "enum": ["temples", "food tour", "museum", "hiking"],
            },
            "minItems": 3,
            "maxItems": 3,
            "uniqueItems": True,
        },
    },
    "required": ["city", "pace", "activities"],
    "additionalProperties": False,
}

PROMPT = """Create a balanced Kyoto itinerary.
Choose exactly three different activities from temples, food tour, museum, and hiking.
Return only the JSON object."""


tokenizer = AutoTokenizer.from_pretrained(MODEL_ID, revision=REVISION)
model = AutoModelForCausalLM.from_pretrained(MODEL_ID, revision=REVISION).eval()

vocabulary = oc_sidememory.Vocabulary.from_transformers(tokenizer)
compiled = oc_sidememory.compile_schema(
    json.dumps(SCHEMA), vocabulary, model.config.vocab_size
)
guide = oc_sidememory.SidememoryGuide(compiled)

messages = [
    {"role": "system", "content": "Return only JSON matching the requested schema."},
    {"role": "user", "content": PROMPT},
]
rendered_prompt = tokenizer.apply_chat_template(
    messages, tokenize=False, add_generation_prompt=True
)
input_ids = tokenizer(rendered_prompt, return_tensors="pt").input_ids
generated = []
past_key_values = None

with torch.inference_mode():
    for _ in range(128):
        result = model(
            input_ids=input_ids,
            past_key_values=past_key_values,
            use_cache=True,
        )
        past_key_values = result.past_key_values

        allowed = torch.tensor(guide.get_tokens(), dtype=torch.long)
        if allowed.numel() == 0:
            raise RuntimeError("schema has no valid continuation")

        logits = result.logits[0, -1]
        token = allowed[torch.argmax(logits[allowed])].item()
        guide.advance(token)

        if token == vocabulary.get_eos_token_id():
            break
        generated.append(token)
        if guide.is_accepting():
            break
        input_ids = torch.tensor([[token]])
    else:
        raise RuntimeError("generation exceeded max_new_tokens")

output = tokenizer.decode(generated)
parsed = json.loads(output)
assert parsed["city"] == "Kyoto"
assert parsed["pace"] == "balanced"
assert len(parsed["activities"]) == len(set(parsed["activities"])) == 3
print(json.dumps(parsed, indent=2))
