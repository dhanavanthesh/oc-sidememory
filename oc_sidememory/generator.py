"""High-level, batch-safe constrained generation for Transformers models."""

from __future__ import annotations

from array import array
from collections.abc import Mapping, Sequence
from typing import Any, Generic, TypeVar, overload

from ._native import SidememoryGuide, compile_schema
from ._output import (
    OutputSpec,
    normalize_extensions,
    normalize_imports,
    normalize_output,
)
from .runtime import Runtime

T = TypeVar("T")
Chat = Sequence[Mapping[str, Any]]
Prompt = str | Chat


class GenerationError(RuntimeError):
    """Generation stopped without producing a complete constrained value."""


_UNSUPPORTED_STRATEGIES: dict[str, Any] = {
    "assistant_model": None,
    "prompt_lookup_num_tokens": None,
    "assistant_early_exit": None,
    "custom_generate": None,
}


def _reject_unsupported(generate_kwargs: dict[str, Any]) -> None:
    enabled = [
        name
        for name, unset in _UNSUPPORTED_STRATEGIES.items()
        if generate_kwargs.get(name, unset) is not unset
    ]
    if enabled:
        raise NotImplementedError(
            "oc-sidememory.Generator does not support: " + ", ".join(sorted(enabled))
        )
    if generate_kwargs.get("num_beams", 1) != 1:
        raise NotImplementedError(
            "beam search is not supported by oc-sidememory.Generator"
        )
    if generate_kwargs.get("num_return_sequences", 1) != 1:
        raise NotImplementedError(
            "num_return_sequences != 1 is not supported by oc-sidememory.Generator"
        )
    for reserved in ("eos_token_id", "pad_token_id"):
        if reserved in generate_kwargs:
            raise NotImplementedError(
                f"{reserved} is resolved from the runtime and cannot be overridden per call"
            )


def _is_chat(value: object) -> bool:
    return (
        isinstance(value, Sequence)
        and not isinstance(value, (str, bytes))
        and bool(value)
        and all(isinstance(message, Mapping) for message in value)
    )


def _normalize_inputs(value: object) -> tuple[bool, list[Prompt]]:
    if isinstance(value, str):
        return True, [value]
    if _is_chat(value):
        return True, [value]  # type: ignore[list-item]
    if not isinstance(value, Sequence) or isinstance(value, (str, bytes)) or not value:
        raise TypeError(
            "prompt must be a string, non-empty chat message list, or non-empty batch"
        )
    if all(isinstance(item, str) for item in value):
        return False, list(value)  # type: ignore[arg-type]
    if all(_is_chat(item) for item in value):
        return False, list(value)  # type: ignore[arg-type]
    raise TypeError(
        "a prompt batch must contain only strings or only chat message lists"
    )


class _BatchMaskProcessor:
    """Keep one guide per row synchronized with Transformers append-only decoding."""

    def __init__(
        self,
        guides: list[SidememoryGuide],
        model_width: int,
        prompt_width: int,
        eos_token_id: int = 0,
    ) -> None:
        import numpy as np
        import torch

        self._guides = guides
        self._model_width = model_width
        self._prompt_width = prompt_width
        self._eos_token_id = eos_token_id
        self._committed_length = 0
        word_count = (model_width + 31) // 32
        self._buffers = [array("I", [0]) * word_count for _ in guides]
        self._views = [
            np.frombuffer(buffer, dtype=np.int32) for buffer in self._buffers
        ]
        self._matrix = np.empty((len(guides), word_count), dtype=np.int32)
        self._tensor = torch.from_numpy(self._matrix)
        self._bit_positions = torch.arange(32, dtype=torch.int32)

    def _advance(self, input_ids: Any) -> None:
        generated_length = int(input_ids.shape[1]) - self._prompt_width
        if generated_length == self._committed_length:
            return
        if generated_length != self._committed_length + 1:
            raise NotImplementedError(
                "oc-sidememory.Generator supports append-only decoding only "
                "(no rewind or multi-token jump)"
            )
        for guide, token_id in zip(self._guides, input_ids[:, -1].tolist()):
            if not guide.is_terminated():
                guide.advance(int(token_id))
        self._committed_length = generated_length

    def __call__(self, input_ids: Any, scores: Any) -> Any:
        import torch

        self._advance(input_ids)
        if tuple(scores.shape) != (len(self._guides), self._model_width):
            raise ValueError(
                f"scores shape {tuple(scores.shape)} does not match guide shape "
                f"({len(self._guides)}, {self._model_width})"
            )
        for row_index, (guide, buffer, view) in enumerate(
            zip(self._guides, self._buffers, self._views)
        ):
            if guide.is_terminated():
                for word_index in range(len(buffer)):
                    buffer[word_index] = 0
                buffer[self._eos_token_id // 32] = 1 << (self._eos_token_id % 32)
            else:
                guide.fill_mask(buffer)
            self._matrix[row_index] = view
            if not view.any():
                raise GenerationError(f"row {row_index} has no valid continuation")

        mask = self._tensor
        if mask.device != scores.device:
            mask = mask.to(scores.device)
        bit_positions = self._bit_positions
        if bit_positions.device != scores.device:
            bit_positions = bit_positions.to(scores.device)
        # int32 is only the transport dtype. Recover the unsigned bit pattern before
        # shifting so a set bit 31 cannot sign-extend into the lower positions.
        words = mask.to(torch.int64) & 0xFFFFFFFF
        allowed = (
            ((words.unsqueeze(-1) >> bit_positions) & 1)
            .bool()
            .view(len(self._guides), -1)[:, : self._model_width]
        )
        scores.masked_fill_(~allowed, -torch.inf)
        return scores


class Generator(Generic[T]):  # noqa: UP046 - Python 3.10/3.11 compatibility
    """Compile once and generate parsed values under semantic constraints."""

    def __init__(
        self,
        runtime: Runtime,
        output: object,
        *,
        extensions: object | None = None,
        imports: object | None = None,
        max_rollback: int = 32,
    ) -> None:
        if not isinstance(max_rollback, int) or isinstance(max_rollback, bool):
            raise TypeError("max_rollback must be an integer")
        if max_rollback < 0:
            raise ValueError("max_rollback must be non-negative")
        self._runtime = runtime
        self._output_spec = normalize_output(output)
        self._imports = normalize_imports(imports)
        self.max_rollback = max_rollback
        self._compiled = compile_schema(
            self._output_spec.schema_text,
            runtime.vocabulary,
            runtime.model_width,
            extensions_json=normalize_extensions(extensions),
        )

    @property
    def output_spec(self) -> OutputSpec[Any]:
        return self._output_spec

    def _render(self, prompt: Prompt) -> str:
        if isinstance(prompt, str):
            return prompt
        return self._runtime.tokenizer.apply_chat_template(
            list(prompt), tokenize=False, add_generation_prompt=True
        )

    def _encode(self, prompts: list[Prompt]) -> tuple[Any, Any]:
        import torch

        tokenizer = self._runtime.tokenizer
        rows = [
            tokenizer(
                self._render(prompt), add_special_tokens=True, return_tensors=None
            )["input_ids"]
            for prompt in prompts
        ]
        empty_rows = [index for index, row in enumerate(rows) if not row]
        if empty_rows:
            raise ValueError(
                f"tokenizer produced an empty prompt for row {empty_rows[0]}"
            )
        width = max(map(len, rows))
        pad_id = tokenizer.pad_token_id
        if pad_id is None:
            pad_id = self._runtime.vocabulary.get_eos_token_id()
        input_ids = torch.full((len(rows), width), int(pad_id), dtype=torch.long)
        attention_mask = torch.zeros((len(rows), width), dtype=torch.long)
        for row_index, row in enumerate(rows):
            start = width - len(row)
            input_ids[row_index, start:] = torch.as_tensor(row, dtype=torch.long)
            attention_mask[row_index, start:] = 1
        return input_ids, attention_mask

    @overload
    def __call__(self, prompt: Prompt, **kwargs: Any) -> T: ...

    @overload
    def __call__(self, prompt: Sequence[Prompt], **kwargs: Any) -> list[T]: ...

    def __call__(
        self,
        prompt: Prompt | Sequence[Prompt],
        *,
        max_new_tokens: int = 128,
        do_sample: bool = False,
        **generate_kwargs: Any,
    ) -> T | list[T]:
        import torch
        from transformers import LogitsProcessorList

        _reject_unsupported(generate_kwargs)
        if not isinstance(max_new_tokens, int) or isinstance(max_new_tokens, bool):
            raise TypeError("max_new_tokens must be an integer")
        if max_new_tokens < 1:
            raise ValueError("max_new_tokens must be positive")
        single, prompts = _normalize_inputs(prompt)
        input_ids, attention_mask = self._encode(prompts)
        prompt_width = int(input_ids.shape[1])
        guides = [
            SidememoryGuide(self._compiled, self.max_rollback, imports=self._imports)
            for _ in prompts
        ]
        eos_id = int(self._runtime.vocabulary.get_eos_token_id())
        processor = _BatchMaskProcessor(
            guides, self._runtime.model_width, prompt_width, eos_id
        )
        supplied = list(generate_kwargs.pop("logits_processor", ()) or ())
        supplied.append(processor)
        generate_kwargs["logits_processor"] = LogitsProcessorList(supplied)

        tokenizer_pad = self._runtime.tokenizer.pad_token_id
        with torch.no_grad():
            output_ids = self._runtime.model.generate(
                input_ids=input_ids,
                attention_mask=attention_mask,
                max_new_tokens=max_new_tokens,
                do_sample=do_sample,
                eos_token_id=eos_id,
                pad_token_id=eos_id if tokenizer_pad is None else tokenizer_pad,
                **generate_kwargs,
            )
        if int(output_ids.shape[0]) != len(prompts):
            raise GenerationError(
                f"model returned {int(output_ids.shape[0])} sequences for "
                f"{len(prompts)} prompts"
            )
        results = [
            self._decode(output_ids[row_index, prompt_width:], row_index)
            for row_index in range(len(prompts))
        ]
        return results[0] if single else results

    def _decode(self, generated: Any, row_index: int) -> T:
        eos_id = int(self._runtime.vocabulary.get_eos_token_id())
        pad_id = self._runtime.tokenizer.pad_token_id
        ids = [int(token) for token in generated.tolist()]
        if eos_id in ids:
            ids = ids[: ids.index(eos_id)]
        while ids and pad_id is not None and ids[-1] == pad_id:
            ids.pop()
        text = self._runtime.tokenizer.decode(
            ids, skip_special_tokens=False, clean_up_tokenization_spaces=False
        )
        try:
            return self._output_spec.parse_json(text)
        except Exception as exc:
            raise GenerationError(
                f"row {row_index} did not produce a complete value within "
                f"max_new_tokens; generated {len(text)} characters"
            ) from exc


__all__ = ["GenerationError", "Generator"]
