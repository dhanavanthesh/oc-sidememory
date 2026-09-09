from ._semantic_helpers import BLUE, COMMA, EOS, GREEN, OPEN, RED, RED_ALIAS, CLOSE, guide


def test_python_methods_expose_one_rust_decision_path():
    decoder = guide()
    for token in [OPEN, RED, COMMA]:
        decoder.advance(token)

    allowed = set(decoder.get_tokens())
    candidates = [RED, RED_ALIAS, GREEN, BLUE]
    assert {token for token in candidates if decoder.probe(token)} == allowed
    assert RED not in allowed
    assert RED_ALIAS not in allowed

    assert decoder.accepts_tokens([GREEN, COMMA, BLUE, CLOSE], finish=True)
    assert not decoder.accepts_tokens([GREEN, COMMA, RED, CLOSE], finish=True)
