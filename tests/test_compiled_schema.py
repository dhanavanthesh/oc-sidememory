import pytest

import oc_sidememory

from ._semantic_helpers import MODEL_WIDTH, compiled, vocabulary


def test_compiled_schema_is_reusable_immutable_input():
    plan = compiled()
    first = oc_sidememory.SidememoryGuide(plan)
    second = oc_sidememory.SidememoryGuide(plan)

    assert plan.get_model_width() == MODEL_WIDTH
    assert first.get_state() == second.get_state()
    assert "constraints=1" in repr(plan)


def test_compile_failure_has_distinct_exception_type():
    with pytest.raises(oc_sidememory.CompileError):
        oc_sidememory.compile_schema("not json", vocabulary(), MODEL_WIDTH)
