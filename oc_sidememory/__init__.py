# Portions derived from outlines-core and modified by OC-Sidememory contributors.
# See PROVENANCE.md and MODIFICATIONS.md.

import sys

# kernels is not reexported as it should remain an optional dependency
from . import _json_schema as json_schema
from ._native import (
    CompiledSchema,
    CompileError,
    Guide,
    ImportedMemory,
    Index,
    InternalInvariantError,
    LifecycleError,
    OcSidememoryError,
    ResourceLimitError,
    RollbackError,
    SemanticViolation,
    SidememoryGuide,
    StructuralRejection,
    Vocabulary,
    compile_schema,
)
from .generator import GenerationError, Generator
from .runtime import Runtime, from_transformers

__all__ = [
    "CompileError",
    "CompiledSchema",
    "GenerationError",
    "Generator",
    "Guide",
    "ImportedMemory",
    "Index",
    "InternalInvariantError",
    "LifecycleError",
    "OcSidememoryError",
    "ResourceLimitError",
    "RollbackError",
    "Runtime",
    "SemanticViolation",
    "SidememoryGuide",
    "StructuralRejection",
    "Vocabulary",
    "compat",
    "compile_schema",
    "from_transformers",
    "json_schema",
]

# Register json_schema so direct submodule imports remain available.
# import ..." works
sys.modules["oc_sidememory.json_schema"] = json_schema

from . import compat as compat
