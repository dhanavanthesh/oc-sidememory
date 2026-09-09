import sys

# kernels is not reexported as it should remain an optional dependency
from . import _json_schema as json_schema
from ._native import (
    CompileError,
    CompiledSchema,
    Guide,
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

__all__ = [
    "CompileError",
    "CompiledSchema",
    "Guide",
    "Index",
    "InternalInvariantError",
    "LifecycleError",
    "OcSidememoryError",
    "ResourceLimitError",
    "RollbackError",
    "SemanticViolation",
    "SidememoryGuide",
    "StructuralRejection",
    "Vocabulary",
    "compile_schema",
    "compat",
    "json_schema",
]

# Register json_schema so direct submodule imports remain available.
# import ..." works
sys.modules["oc_sidememory.json_schema"] = json_schema

from . import compat as compat
