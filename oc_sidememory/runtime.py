"""Reusable Hugging Face Transformers runtime for high-level generation."""

from __future__ import annotations

from dataclasses import dataclass
from itertools import chain
from typing import Any

from ._native import Vocabulary


@dataclass(frozen=True)
class Runtime:
    """A model, tokenizer, vocabulary, and verified logits width."""

    model: Any
    tokenizer: Any
    vocabulary: Vocabulary
    model_width: int


def _logits_vocab_size(model: Any) -> int:
    """Prefer the real output width over configuration metadata."""

    output_embeddings = model.get_output_embeddings()
    if output_embeddings is not None and hasattr(output_embeddings, "weight"):
        return int(output_embeddings.weight.shape[0])
    return int(model.config.vocab_size)


def _require_cpu(model: Any) -> None:
    device_map = getattr(model, "hf_device_map", None)
    if device_map and set(map(str, device_map.values())) - {"cpu"}:
        placements = sorted(set(map(str, device_map.values())))
        raise TypeError(
            "oc-sidememory's Transformers integration runs on CPU only; "
            f"the model is sharded or offloaded across {placements}"
        )

    parameters = model.parameters() if hasattr(model, "parameters") else ()
    buffers = model.buffers() if hasattr(model, "buffers") else ()
    devices = {item.device.type for item in chain(parameters, buffers)}
    if devices - {"cpu"}:
        raise TypeError(
            "oc-sidememory's Transformers integration runs on CPU only; call "
            "model.to('cpu') first"
        )


def from_transformers(model: Any, tokenizer: Any) -> Runtime:
    """Build expensive tokenizer artifacts once for a decoder-only CPU model."""

    if getattr(model.config, "is_encoder_decoder", False):
        raise TypeError(
            "oc-sidememory's Transformers integration supports decoder-only models only"
        )
    _require_cpu(model)
    vocabulary = Vocabulary.from_transformers(tokenizer)
    model_width = _logits_vocab_size(model)
    eos_token_id = int(vocabulary.get_eos_token_id())
    if eos_token_id >= model_width:
        raise ValueError(
            f"tokenizer EOS id {eos_token_id} lies outside model logits width {model_width}"
        )
    return Runtime(model, tokenizer, vocabulary, model_width)


__all__ = ["Runtime", "from_transformers"]
