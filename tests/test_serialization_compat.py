import pytest

from oc_sidememory import Guide, Index, Vocabulary


@pytest.fixture
def values():
    vocabulary = Vocabulary(3, {"1": [1], "2": [2]})
    index = Index(r"[1-9]", vocabulary)
    return [
        (Guide, Guide(index)),
        (Index, index),
        (Vocabulary, vocabulary),
    ]


def encoded_state(value):
    _, arguments = value.__reduce__()
    return bytes(arguments[0])


def test_binary_envelopes_are_versioned_and_round_trip(values):
    for constructor, value in values:
        state = encoded_state(value)
        assert state[:3] == b"OCS"
        assert state[3] == 1
        restored = constructor.from_binary(state)
        assert type(restored) is constructor


@pytest.mark.parametrize(
    "corrupt",
    [
        b"",
        b"legacy-bincode-payload",
        b"OCS\x02I",
        b"OCS\x01?",
    ],
)
def test_unversioned_wrong_version_and_wrong_kind_are_rejected(values, corrupt):
    for constructor, _ in values:
        with pytest.raises(ValueError):
            constructor.from_binary(corrupt)


def test_trailing_bytes_are_rejected(values):
    for constructor, value in values:
        with pytest.raises(ValueError, match="Trailing bytes"):
            constructor.from_binary(encoded_state(value) + b"trailing")
