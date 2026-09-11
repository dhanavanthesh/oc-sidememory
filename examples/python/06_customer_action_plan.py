"""Compose runtime membership, unique actions, contains, and capture equality."""

import json

import oc_sidememory
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer


MODEL_ID = "Qwen/Qwen2.5-0.5B-Instruct"
REVISION = "7ae557604adf67be50417f59c2c2f167def9a775"

SCHEMA = {
    "type": "object",
    "properties": {
        "request_id": {"type": "number"},
        "customer": {"type": "string"},
        "actions": {
            "type": "array",
            "items": {"enum": ["search", "compare", "answer"]},
            "minItems": 3,
            "maxItems": 3,
            "uniqueItems": True,
            "contains": {"const": "answer"},
        },
        "confirmation": {"type": "number"},
    },
    "required": ["request_id", "customer", "actions", "confirmation"],
    "additionalProperties": False,
}

EXTENSIONS = {
    "version": 1,
    "objects": [{
        "schemaPath": "$",
        "propertyOrder": ["request_id", "customer", "actions", "confirmation"],
        "captures": [{"name": "rid", "sourceProperty": "request_id"}],
        "relations": [
            {
                "targetProperty": "customer",
                "operator": "memberOf",
                "import": "live_customers",
            },
            {
                "targetProperty": "confirmation",
                "operator": "equal",
                "capture": "rid",
            },
        ],
    }],
}

PROMPT = """Build an action plan for customer cust_7 and assign a numeric request ID.
Use search, compare, and answer exactly once. Repeat the request ID in confirmation.
Return only the JSON object."""


tokenizer = AutoTokenizer.from_pretrained(MODEL_ID, revision=REVISION)
model = AutoModelForCausalLM.from_pretrained(MODEL_ID, revision=REVISION).eval()
vocabulary = oc_sidememory.Vocabulary.from_transformers(tokenizer)

compiled = oc_sidememory.compile_schema(
    json.dumps(SCHEMA),
    vocabulary,
    model.config.vocab_size,
    extensions_json=json.dumps(EXTENSIONS),
)
imports = oc_sidememory.ImportedMemory.from_json(
    "customer-snapshot", "catalog-v1", '{"live_customers":["cust_7","cust_9"]}'
)
guide = oc_sidememory.SidememoryGuide(compiled, imports=imports, max_rollback=32)

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
    for _ in range(160):
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
assert parsed["customer"] in {"cust_7", "cust_9"}
assert set(parsed["actions"]) == {"search", "compare", "answer"}
assert parsed["confirmation"] == parsed["request_id"]
print(json.dumps(parsed, indent=2))
