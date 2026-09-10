"""Shared cached Transformers setup for the semantic examples."""

import os

os.environ.setdefault("HF_HUB_OFFLINE", "1")
os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")

import torch  # noqa: E402
from transformers import AutoModelForCausalLM, AutoTokenizer  # noqa: E402

import oc_sidememory  # noqa: E402


MODEL_ID = "gpt2"
REVISION = "607a30d783dfa663caf39e06633721c8d4cfcd7e"


def load_runtime():
    tokenizer = AutoTokenizer.from_pretrained(
        MODEL_ID, revision=REVISION, local_files_only=True
    )
    model = AutoModelForCausalLM.from_pretrained(
        MODEL_ID, revision=REVISION, local_files_only=True
    ).cpu().eval()
    vocabulary = oc_sidememory.Vocabulary.from_transformers(tokenizer)
    return model, tokenizer, vocabulary


def accepts(guide, tokenizer, document):
    tokens = tokenizer.encode(document, add_special_tokens=False)
    return guide.accepts_tokens(tokens, finish=True)


def masked_next_token(model, tokenizer, guide):
    prompt = tokenizer("JSON:", return_tensors="pt")
    with torch.no_grad():
        logits = model(**prompt).logits[0, -1]
    allowed = torch.tensor(guide.get_tokens(), dtype=torch.long)
    assert allowed.numel() > 0
    selected = allowed[torch.argmax(logits[allowed])].item()
    assert guide.probe(selected)
    return tokenizer.decode([selected])
