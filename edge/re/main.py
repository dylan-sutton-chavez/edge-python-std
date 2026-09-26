from _re import compile as _compile, findall as _findall, fullmatch as _fullmatch, groups as _groups, match as _match, search as _search, span as _span, sub as _sub

def compile(pattern):
    return _compile(pattern)

def match(pattern, string):
    return _match(pattern, string)

def search(pattern, string):
    return _search(pattern, string)

def fullmatch(pattern, string):
    return _fullmatch(pattern, string)

def findall(pattern, string):
    return _findall(pattern, string)

def groups(pattern, string):
    return _groups(pattern, string)

def span(pattern, string):
    return _span(pattern, string)

def sub(pattern, repl, string):
    return _sub(pattern, repl, string)
