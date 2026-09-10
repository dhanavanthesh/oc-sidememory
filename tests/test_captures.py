import json

import pytest

import oc_sidememory


EOS = 8


def build_guide():
    documents = {
        b'{"id":1,"confirmation":1.0}': [0],
        b'{"id":1,"confirmation":2}': [1],
        b'{"confirmation":1,"id":1}': [2],
    }
    vocabulary = oc_sidememory.Vocabulary(EOS, documents)
    schema = json.dumps(
        {
            "type": "object",
            "properties": {
                "id": {"type": "number"},
                "confirmation": {"type": "number"},
            },
            "required": ["id", "confirmation"],
            "additionalProperties": False,
        }
    )
    extensions = json.dumps(
        {
            "version": 1,
            "objects": [
                {
                    "schemaPath": "$",
                    "propertyOrder": ["id", "confirmation"],
                    "captures": [{"name": "primary", "sourceProperty": "id"}],
                    "relations": [
                        {
                            "targetProperty": "confirmation",
                            "operator": "equal",
                            "capture": "primary",
                        }
                    ],
                }
            ],
        }
    )
    compiled = oc_sidememory.compile_schema(
        schema, vocabulary, EOS + 1, extensions_json=extensions
    )
    return oc_sidememory.SidememoryGuide(compiled)


def test_capture_equality_is_exact_and_ordered():
    guide = build_guide()
    assert guide.probe(0)
    assert not guide.probe(1)
    with pytest.raises(oc_sidememory.StructuralRejection):
        guide.probe(2)

    with pytest.raises(oc_sidememory.SemanticViolation) as caught:
        guide.advance(1)
    assert caught.value.category == "equality_mismatch"
    assert caught.value.property == "confirmation"
    assert caught.value.capture == "primary"
    assert guide.get_allowed_rollback() == 0


def test_extension_compile_failures_use_compile_error():
    vocabulary = oc_sidememory.Vocabulary(EOS, {b"{}": [0]})
    schema = json.dumps(
        {"type": "object", "properties": {}, "additionalProperties": False}
    )
    extensions = json.dumps({"version": 2, "objects": []})
    with pytest.raises(oc_sidememory.CompileError):
        oc_sidememory.compile_schema(
            schema, vocabulary, EOS + 1, extensions_json=extensions
        )
