from types import SimpleNamespace

import pytest
from oc_sidememory.runtime import _logits_vocab_size, from_transformers


class FakeModel:
    config = SimpleNamespace(is_encoder_decoder=False, vocab_size=11)

    def get_output_embeddings(self):
        return SimpleNamespace(weight=SimpleNamespace(shape=(13, 4)))


def test_logits_width_prefers_output_embeddings():
    assert _logits_vocab_size(FakeModel()) == 13


def test_logits_width_falls_back_to_config():
    model = FakeModel()
    model.get_output_embeddings = lambda: None

    assert _logits_vocab_size(model) == 11


def test_encoder_decoder_model_is_rejected_before_vocabulary_build():
    model = FakeModel()
    model.config = SimpleNamespace(is_encoder_decoder=True, vocab_size=11)

    with pytest.raises(TypeError, match="decoder-only"):
        from_transformers(model, object())
