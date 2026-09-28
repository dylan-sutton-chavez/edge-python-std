from json import JSONDecodeError, dumps, loads
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

@test("loads refuses what is not JSON in Python's words and places")
def malformed():
    for text, message in [
        ("tru", "Expecting value: line 1 column 1 (char 0)"),
        ('"abc', "Unterminated string starting at: line 1 column 1 (char 0)"),
        ("01", "Extra data: line 1 column 2 (char 1)"),
        ("1.", "Extra data: line 1 column 2 (char 1)"),
        ("[1,]", "Expecting value: line 1 column 4 (char 3)"),
        ("[1 2]", "Expecting ',' delimiter: line 1 column 4 (char 3)"),
        ("{1:2}", "Expecting property name enclosed in double quotes: line 1 column 2 (char 1)"),
        ('{"a" 1}', "Expecting ':' delimiter: line 1 column 6 (char 5)"),
        ('"a\\qb"', "Invalid \\escape: line 1 column 3 (char 2)"),
        ('"a\\u12G4"', "Invalid \\uXXXX escape: line 1 column 4 (char 3)"),
        ('"a\nb"', "Invalid control character at: line 1 column 3 (char 2)"),
        ("\n\n  [1,\n  x]", "Expecting value: line 4 column 3 (char 10)"),
    ]:
        with raises(JSONDecodeError, match=message):
            loads(text)
    with raises(TypeError, match="the JSON object must be str, bytes or bytearray, not int"):
        loads(5)

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
    assert loads('{"a":1,"b":2}', object_pairs_hook=lambda p: p) == [("a", 1), ("b", 2)]
    assert loads(b'[1]') == [1]

@test("loads keeps its memory flat across many calls")
def many_calls():
    big = "[" + ("1," * 500) + "1]"
    for _ in range(2000):
        loads(big)

@test("dumps writes every kind of value")
def dump_values():
    assert dumps(None) == "null"
    assert dumps('a"b') == '"a\\"b"'
    assert dumps((1, 2)) == "[1, 2]"
    assert dumps({"a": [1, 2.5, "x", None, True]}) == '{"a": [1, 2.5, "x", null, true]}'
    assert dumps([1e16, 1e-07, 0.1, 1.0, -0.0]) == "[1e+16, 1e-07, 0.1, 1.0, -0.0]"

@test("dumps sorts keys and indents")
def layout():
    assert dumps([1, 2], indent=2) == "[\n  1,\n  2\n]"
    assert dumps({"n": 1, "k": "v"}, indent=2, sort_keys=True) == '{\n  "k": "v",\n  "n": 1\n}'
    assert dumps({"a": 1, "b": 2}, separators=(", ", "=")) == '{"a"=1, "b"=2}'
    assert dumps([1], indent="\t") == "[\n\t1\n]"

@test("dumps escapes to ascii unless told not to")
def ascii():
    assert dumps("héllo") == '"h\\u00e9llo"'
    assert dumps("héllo", ensure_ascii=False) == '"héllo"'

@test("dumps writes a number, a bool or None key as a string, and refuses any other unless it may skip it")
def keys():
    assert dumps({1: "a", 1.5: "b", False: "c", None: "d"}) == '{"1": "a", "1.5": "b", "false": "c", "null": "d"}'
    assert dumps({10: "a", 2: "b"}, sort_keys=True) == '{"2": "b", "10": "a"}'
    assert dumps({(1,): 2, "k": 3}, skipkeys=True) == '{"k": 3}'
    with raises(TypeError, match="keys must be str, int, float, bool or None, not tuple"):
        dumps({(1,): 2})
    with raises(TypeError, match="'<' not supported between instances of 'str' and 'int'"):
        dumps({1: "a", "b": 2}, sort_keys=True)

@test("dumps writes NaN and Infinity unless they are refused")
def constants():
    assert dumps(float("nan")) == "NaN" and dumps(float("inf")) == "Infinity"
    with raises(ValueError, match="Out of range float values are not JSON compliant: nan"):
        dumps(float("nan"), allow_nan=False)

@test("dumps asks default for what it cannot write, and refuses it without one")
def default():
    def f():
        pass
    assert dumps({"fn": f}, default=lambda o: "obj") == '{"fn": "obj"}'
    with raises(TypeError, match="Object of type function is not JSON serializable"):
        dumps({"fn": f})
    with raises(TypeError, match="Object of type set is not JSON serializable"):
        dumps({1, 2})

@test("dumps stops a structure that nests too deep")
def circular():
    a = []
    for _ in range(300):
        a = [a]
    with raises(ValueError, match="Circular reference"):
        dumps(a)

run()
