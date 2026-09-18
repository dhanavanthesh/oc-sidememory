"""Normalize high-level output, extension, and imported-memory inputs."""

from __future__ import annotations

import dataclasses
import json
import os
import types
import typing
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any, Generic, TypeVar

from ._native import ImportedMemory

T = TypeVar("T")


@dataclasses.dataclass(frozen=True)
class OutputSpec(Generic[T]):  # noqa: UP046 - Python 3.10/3.11 compatibility
    """A compiled schema's JSON text and matching result parser."""

    schema_text: str
    parse_json: Callable[[str], T]
    output_type: object | None = None


def _parse_json(text: str) -> Any:
    return json.loads(text)


def _looks_like_type(value: object) -> bool:
    return (
        isinstance(value, type)
        or typing.get_origin(value) is not None
        or isinstance(value, types.UnionType)
        or value is None
    )


def _unsupported_output(value: object) -> TypeError:
    return TypeError(
        f"{value!r} is not a supported output; expected a JSON Schema string, dict, "
        "or a Pydantic-compatible type (BaseModel, dataclass, TypedDict, Enum, "
        "Literal, union, or list[T])"
    )


def normalize_output(value: object) -> OutputSpec[Any]:
    """Return one immutable schema/parser pair for a public output argument."""

    if isinstance(value, OutputSpec):
        return value
    if isinstance(value, str):
        # The native compiler owns schema validation and its typed diagnostics.
        return OutputSpec(value, _parse_json)
    if isinstance(value, dict):
        return OutputSpec(
            json.dumps(value, sort_keys=True, ensure_ascii=False), _parse_json
        )
    if not _looks_like_type(value):
        raise _unsupported_output(value)

    try:
        import pydantic
    except ImportError as exc:
        raise ImportError(
            "constraining to a Python type requires Pydantic: "
            'pip install "oc-sidememory[pydantic]"'
        ) from exc

    try:
        adapter: pydantic.TypeAdapter[Any] = pydantic.TypeAdapter(value)
        schema = adapter.json_schema(mode="validation")
    except Exception as exc:
        raise _unsupported_output(value) from exc
    return OutputSpec(
        json.dumps(schema, sort_keys=True, ensure_ascii=False),
        adapter.validate_json,
        value,
    )


def normalize_extensions(value: object | None) -> str | None:
    """Normalize extensions from a mapping, JSON string, or explicit path."""

    if value is None:
        return None
    if isinstance(value, Mapping):
        return json.dumps(value, sort_keys=True, ensure_ascii=False)
    if isinstance(value, os.PathLike):
        return Path(value).read_text(encoding="utf-8")
    if isinstance(value, str):
        return value
    raise TypeError("extensions must be a mapping, JSON string, path, or None")


def normalize_imports(value: object | None) -> ImportedMemory | None:
    """Build imported memory from an explicit snapshot mapping when necessary."""

    if value is None or isinstance(value, ImportedMemory):
        return value
    if not isinstance(value, Mapping) or set(value) != {"identity", "version", "data"}:
        raise TypeError(
            "imports must be ImportedMemory, None, or a mapping containing exactly "
            "identity, version, and data"
        )
    identity = value["identity"]
    version = value["version"]
    data = value["data"]
    if not isinstance(identity, str) or not isinstance(version, str):
        raise TypeError("imports identity and version must be strings")
    if isinstance(data, str):
        data_json = data
    elif isinstance(data, Mapping):
        data_json = json.dumps(data, sort_keys=True, ensure_ascii=False)
    else:
        raise TypeError("imports data must be a mapping or JSON string")
    return ImportedMemory.from_json(identity, version, data_json)


__all__ = ["OutputSpec"]
