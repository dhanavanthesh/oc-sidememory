import json

import oc_sidememory

OPEN = 0
RED = 1
COMMA = 2
GREEN = 3
BLUE = 4
CLOSE = 5
RED_ALIAS = 7
EOS = 8
MODEL_WIDTH = 9


def vocabulary():
    return oc_sidememory.Vocabulary(
        EOS,
        {
            b"[": [OPEN],
            b'"red"': [RED, RED_ALIAS],
            b",": [COMMA],
            b'"green"': [GREEN],
            b'"blue"': [BLUE],
            b"]": [CLOSE],
        },
    )


def schema(unique=True):
    return json.dumps(
        {
            "type": "array",
            "items": {"type": "string", "enum": ["red", "green", "blue"]},
            "minItems": 3,
            "maxItems": 3,
            "uniqueItems": unique,
        }
    )


def compiled(unique=True):
    return oc_sidememory.compile_schema(schema(unique), vocabulary(), MODEL_WIDTH)


def guide(max_rollback=32):
    return oc_sidememory.SidememoryGuide(compiled(), max_rollback=max_rollback)
