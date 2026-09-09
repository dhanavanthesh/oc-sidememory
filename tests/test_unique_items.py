import pytest

import oc_sidememory

from ._semantic_helpers import BLUE, COMMA, OPEN, RED, RED_ALIAS, guide


def test_duplicate_is_removed_and_direct_advance_is_rejected_atomically():
    decoder = guide()
    decoder.advance(OPEN)
    decoder.advance(RED)
    decoder.advance(COMMA)
    state = decoder.get_state()
    rollback = decoder.get_allowed_rollback()

    assert not decoder.probe(RED)
    assert not decoder.probe(RED_ALIAS)
    assert decoder.probe(BLUE)
    with pytest.raises(oc_sidememory.SemanticViolation) as caught:
        decoder.advance(RED_ALIAS)

    assert caught.value.category == "duplicate_array_item"
    assert caught.value.item_index == 1
    assert decoder.get_state() == state
    assert decoder.get_allowed_rollback() == rollback
