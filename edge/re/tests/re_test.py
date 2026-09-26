from re import compile, findall, fullmatch, groups, match, search, span, sub
from test import test, raises, run

@test("match anchors at the start, search looks anywhere, fullmatch takes it all")
def modes():
    assert match(r"\d+", "123abc") == "123" and match(r"\d+", "abc123") is None
    assert search(r"\d+", "abc123def") == "123" and search(r"\d+", "abc") is None
    assert fullmatch(r"\d+", "123") == "123" and fullmatch(r"\d+", "123a") is None

@test("quantifiers are greedy unless made lazy, and braces bound them")
def quantifiers():
    assert search("<.*>", "<a><b>") == "<a><b>"
    assert search("<.*?>", "<a><b>") == "<a>"
    assert search("a{2,3}", "aaaa") == "aaa"

@test("classes, boundaries, alternation and the dot")
def atoms():
    assert search(r"[a-c]+", "zzabcz") == "abc" and search(r"[^0-9]+", "12ab34") == "ab"
    assert search(r"\bword\b", "a word here") == "word"
    assert search("cat|dog|fish", "a dog b") == "dog"
    assert search("a.", "a\nb") is None and search("(?s)a.", "a\nb") is not None
    assert search("(?i)hello", "oh HELLO there") == "HELLO"

@test("backreferences match what their group caught, by number or by name")
def backreferences():
    assert search(r"(\w+) \1", "the the dog") == "the the" and search(r"(\w+) \1", "the dog") is None
    assert search(r"(?P<x>\w+) (?P=x)", "hey hey") == "hey hey"

@test("lookahead and lookbehind check without consuming")
def lookaround():
    assert search(r"\d+(?= dollars)", "pay 100 dollars") == "100"
    assert search(r"\d+(?! dollars)", "pay 100 euros") == "100"
    assert search(r"(?<=\$)\d+", "costs $42 today") == "42"

@test("text is unicode, and a span counts codepoints")
def unicode():
    assert search(r"\w+", "café") == "café"
    assert span(r"\d+", "áé123") == [2, 5]

@test("groups and findall shape their results by the groups in the pattern")
def captures():
    assert groups(r"(\d+)-(\d+)", "x 12-34 y") == ["12", "34"] and groups(r"(\d+)", "abc") is None
    assert findall(r"\d+", "a1 b22 c333") == ["1", "22", "333"]
    assert findall(r"(\d)\d", "12 34 56") == ["1", "3", "5"]

@test("sub expands backreferences in the template")
def substitution():
    assert sub(r"\d+", "#", "a1b22c") == "a#b#c"
    assert sub(r"(\w+)@(\w+)", r"\2.\1", "user@host") == "host.user"
    assert sub(r"(?P<x>\d)", r"[\g<x>]", "a5b") == "a[5]b"

@test("a compiled pattern carries every operation")
def compiled():
    p = compile(r"\d+")
    assert p.findall("a1 b22 c333") == ["1", "22", "333"]
    assert p.search("abc123def") == "123" and p.match("123abc") == "123" and p.match("abc123") is None
    assert p.span("áé123") == [2, 5]
    q = compile(r"(\d+)-(\d+)")
    assert q.groups("x 12-34 y") == ["12", "34"] and q.fullmatch("12-34") == "12-34"
    assert q.sub(r"\2.\1", "a 12-34 b") == "a 34.12 b"

@test("a bad pattern raises ValueError, at compile time too")
def malformed():
    for pattern, message in [("(unbalanced", "missing closing parenthesis"), ("a**", "multiple repeat"), ("(?<=a+)b", "lookbehind requires fixed width"), (r"\1", "invalid group reference"), (r"(a)\2", "invalid group reference"), (r"(\1)", "cannot refer to an open group")]:
        with raises(ValueError, match=message):
            search(pattern, "x")
    with raises(ValueError, match="missing closing parenthesis"):
        compile("(")

@test("backtracking stops with RuntimeError before it runs away")
def backtracking():
    assert len(fullmatch(r"(ab)+", "ab" * 150)) == 300
    with raises(RuntimeError, match="catastrophic backtracking"):
        fullmatch(r"(ab)+", "ab" * 3000)
    with raises(RuntimeError, match="catastrophic backtracking"):
        findall(r"(a+)+$", "a" * 40 + "b")
    with raises(RuntimeError, match="catastrophic backtracking"):
        search("(a+)+$", "a" * 40 + "!")

run()
