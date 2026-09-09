import pickle

import pytest

import oc_sidememory

from ._semantic_helpers import BLUE, CLOSE, COMMA, EOS, GREEN, OPEN, RED, guide


def test_structural_lifecycle_and_rollback_errors_are_distinct():
    decoder = guide()
    with pytest.raises(oc_sidememory.StructuralRejection):
        decoder.advance(EOS)
    with pytest.raises(oc_sidememory.RollbackError):
        decoder.rollback(1)


def test_terminated_guide_rejects_advance_and_live_serialization():
    decoder = guide()
    with pytest.raises(TypeError, match="serialization is not supported"):
        pickle.dumps(decoder)

    for token in [OPEN, RED, COMMA, GREEN, COMMA, BLUE, CLOSE, EOS]:
        decoder.advance(token)
    with pytest.raises(oc_sidememory.LifecycleError):
        decoder.advance(EOS)

    decoder.reset()
    assert not decoder.is_terminated()
