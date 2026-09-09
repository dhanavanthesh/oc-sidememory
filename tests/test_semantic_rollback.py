from ._semantic_helpers import COMMA, OPEN, RED, guide


def test_rollback_and_reset_restore_semantics():
    decoder = guide(max_rollback=8)
    initial = decoder.get_state()
    decoder.advance(OPEN)
    decoder.advance(RED)
    decoder.advance(COMMA)
    assert not decoder.probe(RED)

    decoder.rollback(2)
    assert decoder.probe(RED)
    decoder.rollback(0)
    decoder.reset()
    assert decoder.get_state() == initial
    assert decoder.get_allowed_rollback() == 0
