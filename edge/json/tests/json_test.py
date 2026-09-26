from json import dumps, loads
from test import test, raises, run

@test("loads reads every kind of value")
def values():
    assert loads("null") is None
    assert loads("true") is True
    assert loads("42") == 42
    assert loads("1.5") == 1.5
    assert loads('"a\\nb"') == "a\nb"
    assert loads('{"nested":{"x":[1,2]}}')["nested"]["x"] == [1, 2]

@test("loads reads exponents and the constants")
def numbers():
    assert loads("1e5") == 100000.0 and loads("-0.5e+2") == -50.0
    assert loads("Infinity") == float("inf") and loads("-Infinity") == float("-inf")
    assert loads("NaN") != loads("NaN")

@test("loads refuses what is not JSON")
def malformed():
    with raises(ValueError, match="unknown literal"):
        loads("tru")
    with raises(ValueError, match="unterminated string"):
        loads('"abc')
    with raises(ValueError, match="trailing data"):
        loads("01")
    with raises(ValueError, match="unexpected"):
        loads("[1,]")
    with raises(ValueError, match="object key must be a string"):
        loads("{1:2}")
    for text in ["1.", "1e"]:
        with raises(ValueError, match="invalid number"):
            loads(text)

@test("loads caps nesting at a thousand levels")
def nesting():
    with raises(RuntimeError, match="maximum JSON nesting depth (1000) exceeded"):
        loads("[" * 1001)

@test("loads hands each kind of value to its hook")
def hooks():
    assert loads("42", parse_int=lambda s: "int:" + s) == "int:42"
    assert loads("1.5", parse_float=lambda s: "flt:" + s) == "flt:1.5"
    assert loads("NaN", parse_constant=lambda s: "CONST:" + s) == "CONST:NaN"
    assert loads('{"x":1}', object_hook=lambda d: d["x"]) == 1
    assert loads('{"a":1,"b":2}', object_pairs_hook=lambda p: len(p)) == 2

@test("loads keeps its memory flat across many calls")
def many_calls():
    big = "[" + ("1," * 500) + "1]"
    for _ in range(2000):
        loads(big)

@test("dumps writes every kind of value")
def dump_values():
    assert dumps(None) == "null"
    assert dumps('a"b') == '"a\\"b"'
    assert dumps((1, 2)) == "[1,2]"
    assert dumps({"a": [1, 2.5, "x", None, True]}) == '{"a":[1,2.5,"x",null,true]}'

@test("dumps sorts keys and indents")
def layout():
    assert dumps([1, 2], indent=2) == "[\n  1,\n  2\n]"
    assert dumps({"n": 1, "k": "v"}, indent=2, sort_keys=True) == '{\n  "k": "v",\n  "n": 1\n}'
    assert dumps({"a": 1, "b": 2}, separators=(", ", "=")) == '{"a"=1, "b"=2}'

@test("dumps escapes to ascii unless told not to")
def ascii():
    assert dumps("héllo") == '"h\\u00e9llo"'
    assert dumps("héllo", ensure_ascii=False) == '"héllo"'

@test("dumps refuses a key that is not a string unless it may skip it")
def keys():
    assert dumps({1: 2, "k": 3}, skipkeys=True) == '{"k":3}'
    with raises(TypeError, match="keys must be str"):
        dumps({1: 2})

@test("dumps writes NaN and Infinity unless they are refused")
def constants():
    assert dumps(float("nan")) == "NaN" and dumps(float("inf")) == "Infinity"
    with raises(ValueError, match="Out of range"):
        dumps(float("nan"), allow_nan=False)

@test("dumps asks default for what it cannot write, and refuses it without one")
def default():
    def f():
        pass
    assert dumps({"fn": f}, default=lambda o: "obj") == '{"fn":"obj"}'
    with raises(TypeError, match="'function' is not JSON-serializable"):
        dumps({"fn": f})

@test("dumps stops a structure that nests too deep")
def circular():
    a = []
    for _ in range(300):
        a = [a]
    with raises(ValueError, match="Circular reference"):
        dumps(a)

run()
