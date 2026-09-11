# Modified from outlines-core by OC-Sidememory contributors.
# See PROVENANCE.md and MODIFICATIONS.md.

"""Tests for package imports to catch import/module registration issues."""


def test_import_oc_sidememory():
    import oc_sidememory

    assert hasattr(oc_sidememory, "Guide")
    assert hasattr(oc_sidememory, "Index")
    assert hasattr(oc_sidememory, "Vocabulary")
    assert hasattr(oc_sidememory, "json_schema")


def test_import_json_schema_module():
    from oc_sidememory import json_schema

    assert hasattr(json_schema, "BOOLEAN")
    assert hasattr(json_schema, "build_regex_from_schema")


def test_import_from_json_schema():
    from oc_sidememory.json_schema import (  # noqa: F401
        BOOLEAN,
        DATE,
        DATE_TIME,
        EMAIL,
        INTEGER,
        NULL,
        NUMBER,
        STRING,
        STRING_INNER,
        TIME,
        URI,
        UUID,
        WHITESPACE,
        build_regex_from_schema,
    )

    assert BOOLEAN == "(true|false)"
    assert callable(build_regex_from_schema)


def test_import_main_classes():
    from oc_sidememory import Guide, Index, Vocabulary

    assert Guide is not None
    assert Index is not None
    assert Vocabulary is not None


# The tests below ensure that users can import using
# The native module remains directly importable for packaging diagnostics.


def test_import_rust_extension_directly():
    from oc_sidememory import _native

    assert hasattr(_native, "Guide")
    assert hasattr(_native, "Index")
    assert hasattr(_native, "Vocabulary")
    assert hasattr(_native, "json_schema")


def test_import_from_rust_extension():
    from oc_sidememory._native import Guide, Index, Vocabulary

    assert Guide is not None
    assert Index is not None
    assert Vocabulary is not None


def test_import_json_schema_from_rust_extension():
    from oc_sidememory._native import json_schema

    assert hasattr(json_schema, "BOOLEAN")
    assert hasattr(json_schema, "build_regex_from_schema")
