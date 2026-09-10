import itertools
import json

import oc_sidememory


def test_contains_finite_domain_matches_independent_oracle_and_mask():
    documents = []
    expected = []
    for length in range(4):
        for values in itertools.product(("a", "b", "c"), repeat=length):
            documents.append(json.dumps(values, separators=(",", ":")).encode())
            matches = sum(value == "a" for value in values)
            expected.append(1 <= matches <= 2)

    eos = len(documents) + 5
    vocabulary = oc_sidememory.Vocabulary(
        eos, {document: [index] for index, document in enumerate(documents)}
    )
    schema = json.dumps(
        {
            "type": "array",
            "items": {"type": "string", "enum": ["a", "b", "c"]},
            "maxItems": 3,
            "contains": {"const": "a"},
            "minContains": 1,
            "maxContains": 2,
        }
    )
    guide = oc_sidememory.SidememoryGuide(
        oc_sidememory.compile_schema(schema, vocabulary, eos + 1)
    )

    allowed = set(guide.get_tokens())
    mask = guide.get_mask()
    from_mask = {
        token
        for token in range(eos + 1)
        if mask[token // 32] & (1 << (token % 32))
    }
    oracle = {index for index, decision in enumerate(expected) if decision}

    assert allowed == from_mask == oracle
    assert [guide.probe(index) for index in range(len(documents))] == expected
    assert sum(expected) == 24
