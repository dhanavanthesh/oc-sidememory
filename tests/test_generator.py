import json
from types import SimpleNamespace

import pytest
from oc_sidememory.generator import (
    GenerationError,
    Generator,
    _BatchMaskProcessor,
    _normalize_inputs,
    _reject_unsupported,
)


class FakeGuide:
    def __init__(self, masks):
        self.masks = list(masks)
        self.advanced = []
        self.terminated = False

    def fill_mask(self, buffer):
        mask = self.masks[len(self.advanced)]
        for index, word in enumerate(mask):
            buffer[index] = word

    def advance(self, token):
        self.advanced.append(token)

    def is_terminated(self):
        return self.terminated


def test_input_shapes_distinguish_prompt_chat_and_batches():
    chat = [{"role": "user", "content": "hello"}]
    chat_two = [{"role": "user", "content": "world"}]

    assert _normalize_inputs("hello") == (True, ["hello"])
    assert _normalize_inputs(["a", "b"]) == (False, ["a", "b"])
    assert _normalize_inputs(chat) == (True, [chat])
    assert _normalize_inputs([chat, chat_two]) == (False, [chat, chat_two])


@pytest.mark.parametrize(
    ("kwargs", "message"),
    [
        ({"num_beams": 2}, "beam search"),
        ({"num_return_sequences": 2}, "num_return_sequences"),
        ({"assistant_model": object()}, "assistant_model"),
        ({"prompt_lookup_num_tokens": 2}, "prompt_lookup_num_tokens"),
        ({"eos_token_id": 1}, "eos_token_id"),
    ],
)
def test_unsupported_generation_modes_are_rejected(kwargs, message):
    with pytest.raises(NotImplementedError, match=message):
        _reject_unsupported(kwargs)


def test_batch_processor_fills_exact_masks_and_advances_one_token_per_step():
    torch = pytest.importorskip("torch")
    guides = [FakeGuide([[0b0011], [0b0100]]), FakeGuide([[0b0101], [0b1000]])]
    processor = _BatchMaskProcessor(guides, model_width=4, prompt_width=2)
    scores = torch.zeros((2, 4))

    processor(torch.tensor([[9, 9], [8, 8]]), scores)
    assert scores.isfinite().tolist() == [
        [True, True, False, False],
        [True, False, True, False],
    ]

    scores.zero_()
    processor(torch.tensor([[9, 9, 1], [8, 8, 2]]), scores)
    assert guides[0].advanced == [1]
    assert guides[1].advanced == [2]
    assert scores.isfinite().tolist() == [
        [False, False, True, False],
        [False, False, False, True],
    ]


def test_batch_processor_rejects_multi_token_jump():
    torch = pytest.importorskip("torch")
    processor = _BatchMaskProcessor([FakeGuide([[1]])], model_width=1, prompt_width=1)

    with pytest.raises(NotImplementedError, match="append-only"):
        processor(torch.tensor([[9, 0, 0]]), torch.zeros((1, 1)))


def test_batch_processor_treats_sign_bit_as_data_not_sign():
    torch = pytest.importorskip("torch")
    processor = _BatchMaskProcessor(
        [FakeGuide([[0x80000000]])], model_width=32, prompt_width=1
    )
    scores = torch.zeros((1, 32))

    processor(torch.tensor([[9]]), scores)

    assert scores.isfinite().nonzero().tolist() == [[0, 31]]


def test_batch_processor_uses_eos_only_mask_for_a_finished_row():
    torch = pytest.importorskip("torch")
    finished = FakeGuide([[0]])
    finished.terminated = True
    active = FakeGuide([[0b0010]])
    processor = _BatchMaskProcessor(
        [finished, active], model_width=4, prompt_width=1, eos_token_id=3
    )
    scores = torch.zeros((2, 4))

    processor(torch.tensor([[9], [9]]), scores)

    assert scores.isfinite().tolist() == [
        [False, False, False, True],
        [False, True, False, False],
    ]


def test_generator_compiles_once_and_forwards_semantic_options(monkeypatch):
    calls = []

    def fake_compile(schema, vocabulary, width, *, extensions_json=None):
        calls.append((schema, vocabulary, width, extensions_json))
        return "compiled"

    monkeypatch.setattr("oc_sidememory.generator.compile_schema", fake_compile)
    runtime = SimpleNamespace(
        model=object(), tokenizer=object(), vocabulary="vocab", model_width=9
    )
    extensions = {"version": 1, "objects": []}

    generator = Generator(
        runtime, {"type": "string"}, extensions=extensions, max_rollback=7
    )

    assert len(calls) == 1
    assert json.loads(calls[0][0]) == {"type": "string"}
    assert calls[0][1:3] == ("vocab", 9)
    assert json.loads(calls[0][3]) == extensions
    assert generator.max_rollback == 7


def test_generator_renders_chat_with_generation_prompt(monkeypatch):
    monkeypatch.setattr(
        "oc_sidememory.generator.compile_schema", lambda *args, **kwargs: "compiled"
    )

    class Tokenizer:
        def apply_chat_template(self, messages, *, tokenize, add_generation_prompt):
            assert messages == [{"role": "user", "content": "hello"}]
            assert tokenize is False
            assert add_generation_prompt is True
            return "rendered chat"

    runtime = SimpleNamespace(
        model=object(), tokenizer=Tokenizer(), vocabulary="vocab", model_width=9
    )
    generator = Generator(runtime, {"type": "string"})

    assert generator._render([{"role": "user", "content": "hello"}]) == "rendered chat"


def test_generator_rejects_prompt_that_tokenizes_to_nothing(monkeypatch):
    monkeypatch.setattr(
        "oc_sidememory.generator.compile_schema", lambda *args, **kwargs: "compiled"
    )

    class Tokenizer:
        pad_token_id = None

        def __call__(self, *args, **kwargs):
            return {"input_ids": []}

    runtime = SimpleNamespace(
        model=object(), tokenizer=Tokenizer(), vocabulary="vocab", model_width=9
    )
    generator = Generator(runtime, {"type": "string"})

    with pytest.raises(ValueError, match="empty prompt for row 0"):
        generator._encode([""])


def test_generation_error_is_public_exception():
    assert issubclass(GenerationError, RuntimeError)
