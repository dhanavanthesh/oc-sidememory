"""Generate a schema-constrained travel itinerary with Qwen."""

import oc_sidememory
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
model = AutoModelForCausalLM.from_pretrained(MODEL_ID, revision=REVISION).cpu().eval()

messages = [
    {"role": "system", "content": "Return only JSON matching the requested schema."},
    {"role": "user", "content": PROMPT},
]
runtime = oc_sidememory.from_transformers(model, tokenizer)
generate = oc_sidememory.Generator(runtime, SCHEMA)
itinerary = generate(messages, max_new_tokens=128)

assert itinerary["city"] in {"Kyoto", "Paris", "Reykjavik"}
assert itinerary["pace"] in {"relaxed", "balanced", "busy"}
assert len(itinerary["activities"]) == len(set(itinerary["activities"])) == 3
print(itinerary)
