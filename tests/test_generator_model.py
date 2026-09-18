"""Offline real-model coverage for the high-level Transformers facade."""

import pytest


@pytest.mark.no_cover
def test_cached_gpt2_generates_parsed_unique_array():
    transformers = pytest.importorskip("transformers")

    import oc_sidememory

    model_id = "gpt2"
    revision = "607a30d783dfa663caf39e06633721c8d4cfcd7e"
    try:
        tokenizer = transformers.AutoTokenizer.from_pretrained(
            model_id, revision=revision, local_files_only=True
        )
        model = (
            transformers.AutoModelForCausalLM.from_pretrained(
                model_id, revision=revision, local_files_only=True
            )
            .cpu()
            .eval()
        )
    except OSError:
        pytest.skip("the pinned GPT-2 checkpoint is not cached")

    schema = {
        "type": "array",
        "items": {"type": "string", "enum": ["red", "green", "blue"]},
        "minItems": 3,
        "maxItems": 3,
        "uniqueItems": True,
    }
    runtime = oc_sidememory.from_transformers(model, tokenizer)
    generate = oc_sidememory.Generator(runtime, schema)

    prompt = 'Return this JSON array exactly: ["red","green","blue"]\nJSON:'
    result = generate(prompt, max_new_tokens=64)

    assert isinstance(result, list)
    assert len(result) == len(set(result)) == 3
    assert set(result) == {"red", "green", "blue"}

    batch = generate([prompt, prompt], max_new_tokens=64)
    assert batch == [result, result]

    composed_schema = {
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
    extensions = {
        "version": 1,
        "objects": [
            {
                "schemaPath": "$",
                "propertyOrder": [
                    "request_id",
                    "customer",
                    "actions",
                    "confirmation",
                ],
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
            }
        ],
    }
    imports = {
        "identity": "test-snapshot",
        "version": "v1",
        "data": {"live_customers": ["cust_7"]},
    }
    generate_composed = oc_sidememory.Generator(
        runtime, composed_schema, extensions=extensions, imports=imports
    )
    composed_prompt = (
        "Return this JSON object exactly: "
        '{"request_id":1,"customer":"cust_7","actions":'
        '["search","compare","answer"],"confirmation":1}\nJSON:'
    )

    composed = generate_composed(composed_prompt, max_new_tokens=128)

    assert composed["customer"] == "cust_7"
    assert set(composed["actions"]) == {"search", "compare", "answer"}
    assert composed["confirmation"] == composed["request_id"]
