import json

import pytest

import oc_sidememory


EOS = 9


def build_guide():
    documents = [b"[]", b"[1]", b"[2]", b"[1,2]", b"[1,1]"]
    vocabulary = oc_sidememory.Vocabulary(
        EOS, {document: [index] for index, document in enumerate(documents)}
    )
    schema = json.dumps(
        {
            "type": "array",
            "items": {"type": "integer"},
            "maxItems": 2,
            "contains": {"const": 1},
            "minContains": 1,
            "maxContains": 1,
        }
    )
    compiled = oc_sidememory.compile_schema(schema, vocabulary, EOS + 1)
    return oc_sidememory.SidememoryGuide(compiled)


def test_contains_filters_complete_candidates_and_preserves_state():
    guide = build_guide()
    state = guide.get_state()
    rollback = guide.get_allowed_rollback()

    assert not guide.probe(0)
    assert guide.probe(1)
    assert not guide.probe(2)
    assert guide.probe(3)
    assert not guide.probe(4)
    assert guide.get_state() == state
    assert guide.get_allowed_rollback() == rollback


def test_contains_violations_keep_structured_count_fields():
    guide = build_guide()
    with pytest.raises(oc_sidememory.SemanticViolation) as caught:
        guide.advance(4)

    assert caught.value.category == "contains_maximum_exceeded"
    assert caught.value.item_index == 1
    assert caught.value.processed == 2
    assert caught.value.matched == 2
    assert caught.value.upper == 1
    assert guide.get_allowed_rollback() == 0

    with pytest.raises(oc_sidememory.SemanticViolation) as caught:
        guide.advance(2)
    assert caught.value.category == "contains_minimum_not_met"
    assert caught.value.processed == 1
    assert caught.value.matched == 0
    assert caught.value.lower == 1


def test_min_and_max_contains_are_inert_without_contains():
    vocabulary = oc_sidememory.Vocabulary(EOS, {b"[2]": [0]})
    schema = json.dumps(
        {
            "type": "array",
            "items": {"type": "integer"},
            "minContains": 4,
            "maxContains": 0,
        }
    )
    compiled = oc_sidememory.compile_schema(schema, vocabulary, EOS + 1)
    guide = oc_sidememory.SidememoryGuide(compiled)
    assert guide.probe(0)
