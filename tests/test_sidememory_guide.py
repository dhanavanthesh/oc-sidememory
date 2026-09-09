from ._semantic_helpers import BLUE, COMMA, EOS, GREEN, OPEN, RED, CLOSE, guide


def test_complete_unique_document_lifecycle():
    decoder = guide()
    for token in [OPEN, RED, COMMA, GREEN, COMMA, BLUE, CLOSE]:
        decoder.advance(token)

    assert decoder.is_accepting()
    assert not decoder.is_terminated()
    decoder.advance(EOS)
    assert decoder.is_accepting()
    assert decoder.is_terminated()
    assert decoder.get_tokens() == []


def test_accepts_tokens_is_observationally_pure():
    decoder = guide()
    state = decoder.get_state()
    assert decoder.accepts_tokens(
        [OPEN, RED, COMMA, GREEN, COMMA, BLUE, CLOSE], finish=True
    )
    assert not decoder.accepts_tokens(
        [OPEN, RED, COMMA, RED, COMMA, BLUE, CLOSE], finish=True
    )
    assert decoder.get_state() == state
    assert decoder.get_allowed_rollback() == 0
