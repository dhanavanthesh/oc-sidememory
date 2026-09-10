import json

import pytest

import oc_sidememory


EOS = 7


def compile_membership():
    vocabulary = oc_sidememory.Vocabulary(
        EOS,
        {
            b'{"customer":"cust_7"}': [0],
            b'{"customer":"cust_8"}': [1],
        },
    )
    schema = json.dumps(
        {
            "type": "object",
            "properties": {"customer": {"type": "string"}},
            "required": ["customer"],
            "additionalProperties": False,
        }
    )
    extensions = json.dumps(
        {
            "version": 1,
            "objects": [
                {
                    "schemaPath": "$",
                    "propertyOrder": ["customer"],
                    "relations": [
                        {
                            "targetProperty": "customer",
                            "operator": "memberOf",
                            "import": "valid_customers",
                        }
                    ],
                }
            ],
        }
    )
    return oc_sidememory.compile_schema(
        schema, vocabulary, EOS + 1, extensions_json=extensions
    )


def test_imported_memory_metadata_and_membership():
    imported = oc_sidememory.ImportedMemory.from_json(
        "customer-catalog",
        "2026-09-10",
        '{"valid_customers":["cust_7","cust_9"]}',
    )
    assert imported.get_identity() == "customer-catalog"
    assert imported.get_version() == "2026-09-10"
    assert imported.get_set_count() == 1

    guide = oc_sidememory.SidememoryGuide(compile_membership(), imports=imported)
    assert guide.probe(0)
    assert not guide.probe(1)
    with pytest.raises(oc_sidememory.SemanticViolation) as caught:
        guide.advance(1)
    assert caught.value.category == "imported_member_required"
    assert caught.value.property == "customer"
    assert caught.value.import_set == "valid_customers"


def test_required_import_and_metadata_errors_are_explicit():
    with pytest.raises(oc_sidememory.OcSidememoryError, match="required imported set"):
        oc_sidememory.SidememoryGuide(compile_membership())
    with pytest.raises(ValueError, match="non-empty"):
        oc_sidememory.ImportedMemory.from_json("", "v1", "{}")
