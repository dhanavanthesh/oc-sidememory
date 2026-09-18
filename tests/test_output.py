import json
from dataclasses import dataclass

import pytest
from oc_sidememory._output import (
    normalize_extensions,
    normalize_imports,
    normalize_output,
)


def test_dict_output_round_trips_json():
    spec = normalize_output({"type": "array", "items": {"type": "integer"}})

    assert json.loads(spec.schema_text)["type"] == "array"
    assert spec.parse_json("[1, 2]") == [1, 2]


def test_schema_string_is_left_for_native_compiler_to_validate():
    spec = normalize_output("not json")

    assert spec.schema_text == "not json"


def test_pydantic_compatible_type_returns_typed_value():
    pytest.importorskip("pydantic")

    @dataclass
    class Result:
        value: int

    spec = normalize_output(Result)

    assert json.loads(spec.schema_text)["type"] == "object"
    assert spec.parse_json('{"value": 7}') == Result(value=7)


def test_arbitrary_instance_is_not_treated_as_a_pydantic_type():
    with pytest.raises(TypeError, match="supported output"):
        normalize_output(object())


def test_extensions_accept_dict_json_and_path(tmp_path):
    value = {"version": 1, "objects": []}
    path = tmp_path / "extensions.json"
    path.write_text(json.dumps(value), encoding="utf-8")

    assert json.loads(normalize_extensions(value)) == value
    assert json.loads(normalize_extensions(json.dumps(value))) == value
    assert json.loads(normalize_extensions(path)) == value


def test_import_mapping_has_explicit_identity_version_and_data(monkeypatch):
    calls = []

    class FakeImportedMemory:
        @staticmethod
        def from_json(identity, version, data):
            calls.append((identity, version, data))
            return "memory"

    monkeypatch.setattr("oc_sidememory._output.ImportedMemory", FakeImportedMemory)

    result = normalize_imports(
        {"identity": "request-7", "version": "v2", "data": {"ids": ["a"]}}
    )

    assert result == "memory"
    assert calls == [("request-7", "v2", '{"ids": ["a"]}')]


def test_import_mapping_rejects_ambiguous_shape():
    with pytest.raises(TypeError, match="identity.*version.*data"):
        normalize_imports({"ids": ["a"]})
