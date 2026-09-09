from array import array

import pytest

from ._semantic_helpers import BLUE, COMMA, GREEN, RED, RED_ALIAS, OPEN, guide


def mask_tokens(words):
    return {
        token
        for token in range(9)
        if words[token // 32] & (1 << (token % 32))
    }


def test_list_and_buffer_masks_match_allowed_tokens():
    decoder = guide()
    for token in [OPEN, RED, COMMA]:
        decoder.advance(token)

    allowed = set(decoder.get_tokens())
    returned = decoder.get_mask()
    output = array("I", [0xFFFFFFFF])
    decoder.fill_mask(output)

    assert mask_tokens(returned) == allowed
    assert list(output) == returned
    assert RED not in allowed
    assert RED_ALIAS not in allowed
    assert GREEN in allowed
    assert BLUE in allowed


def test_fill_mask_validates_writable_uint32_shape():
    decoder = guide()
    with pytest.raises(TypeError, match="writable"):
        decoder.fill_mask(bytes(4))
    with pytest.raises(TypeError, match="unsigned 32-bit"):
        decoder.fill_mask(bytearray(4))
    with pytest.raises(ValueError, match="expected 1 words"):
        decoder.fill_mask(array("I", [0, 0]))
