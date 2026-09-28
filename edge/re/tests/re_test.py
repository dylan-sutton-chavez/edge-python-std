import re
from test import test, raises, run

@test("match anchors at the start, search looks anywhere, fullmatch takes it all")
def modes():
    assert re.match(r"\d+", "123abc")[0] == "123" and re.match(r"\d+", "abc123") is None
    assert re.search(r"\d+", "abc123def")[0] == "123" and re.search(r"\d+", "abc") is None
    assert re.fullmatch(r"\d+", "123")[0] == "123" and re.fullmatch(r"\d+", "123a") is None

@test("quantifiers are greedy unless made lazy, and braces bound them")
def quantifiers():
    assert re.search("<.*>", "<a><b>")[0] == "<a><b>"
    assert re.search("<.*?>", "<a><b>")[0] == "<a>"
    assert re.search("a{2,3}", "aaaa")[0] == "aaa"

@test("classes, boundaries, alternation and the dot")
def atoms():
    assert re.search(r"[a-c]+", "zzabcz")[0] == "abc" and re.search(r"[^0-9]+", "12ab34")[0] == "ab"
    assert re.search(r"\bword\b", "a word here")[0] == "word"
    assert re.search("cat|dog|fish", "a dog b")[0] == "dog"
    assert re.search("a.", "a\nb") is None and re.search("(?s)a.", "a\nb") is not None
    assert re.search("(?i)hello", "oh HELLO there")[0] == "HELLO"

@test("flags work like the inline groups")
def flags():
    assert re.search("hello", "oh HELLO", re.I)[0] == "HELLO"
    assert re.findall("^b", "a\nb", re.M) == ["b"] and re.findall("^b", "a\nb") == []
    assert re.search("a.b", "A\nB", re.S | re.I) is not None
    assert re.compile("x", re.I | re.M).flags == re.I | re.M | re.U

@test("backreferences match what their group caught, by number or by name")
def backreferences():
    assert re.search(r"(\w+) \1", "the the dog")[0] == "the the" and re.search(r"(\w+) \1", "the dog") is None
    assert re.search(r"(?P<x>\w+) (?P=x)", "hey hey")[0] == "hey hey"

@test("lookahead and lookbehind check without consuming")
def lookaround():
    assert re.search(r"\d+(?= dollars)", "pay 100 dollars")[0] == "100"
    assert re.search(r"\d+(?! dollars)", "pay 100 euros")[0] == "100"
    assert re.search(r"(?<=\$)\d+", "costs $42 today")[0] == "42"

@test("a match carries its groups, their spans and their names")
def matches():
    m = re.search(r"(\d+)-(\d+)", "tel 12-34")
    assert m.group() == "12-34" and m.group(1) == "12" and m[2] == "34"
    assert m.group(1, 2) == ("12", "34") and m.groups() == ("12", "34")
    assert m.span() == (4, 9) and m.start(2) == 7 and m.end(1) == 6
    assert repr(m) == "<re.Match object; span=(4, 9), match='12-34'>"
    n = re.match(r"(?P<user>\w+)@(?P<host>\w+)", "ana@mail")
    assert n["user"] == "ana" and n.group("host") == "mail" and n.groupdict() == {"user": "ana", "host": "mail"}
    o = re.match(r"(a)|(b)", "b")
    assert o.groups() == (None, "b") and o.groups("-") == ("-", "b") and o.span(1) == (-1, -1)
    with raises(IndexError):
        m.group(3)

@test("an empty match is still a match")
def empty():
    assert re.search(r"x*", "abc") is not None and re.search(r"x*", "abc")[0] == ""
    assert [m.span() for m in re.finditer(r"\b|a", "a")] == [(0, 0), (0, 1), (1, 1)]

@test("text is unicode, and a span counts codepoints")
def unicode():
    assert re.search(r"\w+", "café")[0] == "café"
    assert re.search(r"\d+", "áé123").span() == (2, 5)

@test("findall shapes its results by the groups in the pattern, finditer yields matches")
def iteration():
    assert re.findall(r"\d+", "a1 b22 c333") == ["1", "22", "333"]
    assert re.findall(r"(\d)\d", "12 34 56") == ["1", "3", "5"]
    assert re.findall(r"(\w)=(\d)?", "a=1 b=") == [("a", "1"), ("b", "")]
    assert [(m[0], m.start()) for m in re.finditer(r"\d+", "a1 b22")] == [("1", 1), ("22", 4)]

@test("sub expands a template or calls a function, and count caps it")
def substitution():
    assert re.sub(r"\d+", "#", "a1b22c") == "a#b#c"
    assert re.sub(r"(\w+)@(\w+)", r"\2.\1", "user@host") == "host.user"
    assert re.sub(r"(?P<x>\d)", r"[\g<x>]", "a5b") == "a[5]b"
    assert re.sub(r"\d+", lambda m: str(int(m[0]) * 2), "a1 b22") == "a2 b44"
    assert re.sub(r"\d", "#", "a1b2c3", count=2) == "a#b#c3"
    assert re.subn(r"\d", "#", "a1b2c3") == ("a#b#c#", 3)
    assert re.sub("x*", "-", "abxd") == "-a-b--d-"

@test("split cuts at every match and keeps what groups caught")
def splitting():
    assert re.split(r"[,;]\s*", "a, b;c") == ["a", "b", "c"]
    assert re.split(r"(,)", "a,b") == ["a", ",", "b"]
    assert re.split(r",", "a,b,c", maxsplit=1) == ["a", "b,c"]
    assert re.split(r"x*", "axbc") == ["", "a", "", "b", "c", ""]

@test("a compiled pattern carries every operation and describes itself")
def compiled():
    p = re.compile(r"(\d+)-(\d+)")
    assert p.groups == 2 and p.pattern == r"(\d+)-(\d+)" and p.groupindex == {}
    assert p.search("x 12-34 y").groups() == ("12", "34") and p.fullmatch("12-34") is not None
    assert p.sub(r"\2.\1", "a 12-34 b") == "a 34.12 b"
    assert re.compile(r"(?P<n>\d)").groupindex == {"n": 1}
    assert repr(re.compile("hola", re.I)) == "re.compile('hola', re.IGNORECASE)"
    assert repr(p) == r"re.compile('(\\d+)-(\\d+)')" and re.compile(p) is p

@test("escape guards every character a pattern gives meaning to")
def escaping():
    assert re.escape("a.b*c") == r"a\.b\*c" and re.escape("x_1") == "x_1"
    assert re.fullmatch(re.escape("1+1=2?"), "1+1=2?") is not None

@test("a bad pattern raises re.error, at compile time too")
def malformed():
    for pattern in ["(unbalanced", "a**", "(?<=a+)b", r"\1", r"(a)\2", r"(\1)"]:
        with raises(re.error):
            re.search(pattern, "x")
    with raises(re.error):
        re.compile("(")

@test("a bad pattern names what is wrong")
def messages():
    for pattern, message in [("(unbalanced", "missing closing parenthesis"), ("a**", "multiple repeat"), ("(?<=a+)b", "lookbehind requires fixed width"), (r"\1", "invalid group reference"), (r"(a)\2", "invalid group reference"), (r"(\1)", "cannot refer to an open group")]:
        with raises(re.error, match=message):
            re.search(pattern, "x")

@test("backtracking stops with RuntimeError before it runs away")
def backtracking():
    assert len(re.fullmatch(r"(ab)+", "ab" * 150)[0]) == 300
    with raises(RuntimeError, match="catastrophic backtracking"):
        re.fullmatch(r"(ab)+", "ab" * 3000)
    with raises(RuntimeError, match="catastrophic backtracking"):
        re.findall(r"(a+)+$", "a" * 40 + "b")
    with raises(RuntimeError, match="catastrophic backtracking"):
        re.search("(a+)+$", "a" * 40 + "!")

run()
