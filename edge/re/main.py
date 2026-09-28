from _re import find as _find, find_all as _find_all, info as _info, sub as _sub

# The flag values Python uses, so combined flags read the same.
I = IGNORECASE = 2
M = MULTILINE = 8
S = DOTALL = 16
U = UNICODE = 32

# A bad pattern raises ValueError, the one exception class a program can catch it as.
error = ValueError

# Each flag becomes the inline group the engine reads at the start of a pattern.
_INLINE = ((I, "i"), (M, "m"), (S, "s"))
_NAMES = ((I, "IGNORECASE"), (M, "MULTILINE"), (S, "DOTALL"))

# The characters a pattern gives meaning to, each escaped with a backslash.
_SPECIAL = "()[]{}?*+-|^$\\.&~# \t\n\r\v\f"

class Match:
    def __init__(self, pattern, string, spans):
        self.re = pattern
        self.string = string
        self.pos = 0
        self.endpos = len(string)
        self._spans = spans

    def _index(self, group):
        if isinstance(group, str) and group in self.re.groupindex:
            return self.re.groupindex[group]
        if isinstance(group, int) and 0 <= group <= self.re.groups:
            return group
        raise IndexError("no such group")

    def _text(self, index, default=None):
        start, end = self._spans[2 * index], self._spans[2 * index + 1]
        return default if start < 0 else self.string[start:end]

    def span(self, group=0):
        index = self._index(group)
        return (self._spans[2 * index], self._spans[2 * index + 1])

    def start(self, group=0):
        return self.span(group)[0]

    def end(self, group=0):
        return self.span(group)[1]

    def group(self, *groups):
        if len(groups) == 0:
            return self._text(0)
        if len(groups) == 1:
            return self._text(self._index(groups[0]))
        return tuple(self._text(self._index(g)) for g in groups)

    def __getitem__(self, group):
        return self._text(self._index(group))

    def groups(self, default=None):
        return tuple(self._text(i, default) for i in range(1, self.re.groups + 1))

    def groupdict(self, default=None):
        return {name: self._text(index, default) for name, index in self.re.groupindex.items()}

    def __repr__(self):
        return f"<re.Match object; span={self.span()}, match={repr(self.group())}>"

class Pattern:
    def __init__(self, pattern, flags):
        self.pattern = pattern
        self.flags = flags | UNICODE
        inline = "".join(letter for flag, letter in _INLINE if flags & flag)
        self._source = f"(?{inline}){pattern}" if inline else pattern
        self.groups, self.groupindex = _info(self._source)

    def _one(self, string, mode):
        spans = _find(self._source, string, mode)
        return None if spans is None else Match(self, string, spans)

    def search(self, string):
        return self._one(string, 0)

    def match(self, string):
        return self._one(string, 1)

    def fullmatch(self, string):
        return self._one(string, 2)

    # Every match up to `limit`, zero meaning all, which sub and split cap by their count.
    def _all(self, string, limit):
        for spans in _find_all(self._source, string, limit):
            yield Match(self, string, spans)

    def finditer(self, string):
        return self._all(string, 0)

    def findall(self, string):
        if self.groups == 0:
            return [m.group() for m in self.finditer(string)]
        if self.groups == 1:
            return [m.group(1) or "" for m in self.finditer(string)]
        return [m.groups("") for m in self.finditer(string)]

    def subn(self, repl, string, count=0):
        if not callable(repl):
            text, n = _sub(self._source, repl, string, count)
            return (text, n)
        pieces, last, n = [], 0, 0
        for m in self._all(string, count):
            pieces.append(string[last:m.start()])
            pieces.append(repl(m))
            last = m.end()
            n += 1
        pieces.append(string[last:])
        return ("".join(pieces), n)

    def sub(self, repl, string, count=0):
        return self.subn(repl, string, count)[0]

    def split(self, string, maxsplit=0):
        pieces, last = [], 0
        for m in self._all(string, maxsplit):
            pieces.append(string[last:m.start()])
            pieces.extend(m.groups())
            last = m.end()
        pieces.append(string[last:])
        return pieces

    def __repr__(self):
        named = "|".join(f"re.{name}" for flag, name in _NAMES if self.flags & flag)
        return f"re.compile({repr(self.pattern)}, {named})" if named else f"re.compile({repr(self.pattern)})"

# Patterns compiled lately stay compiled, so a loop over re.search compiles once.
_cache = {}

def compile(pattern, flags=0):
    if isinstance(pattern, Pattern):
        return pattern
    key = (pattern, flags)
    if key not in _cache:
        if len(_cache) >= 64:
            _cache.clear()
        _cache[key] = Pattern(pattern, flags)
    return _cache[key]

def search(pattern, string, flags=0):
    return compile(pattern, flags).search(string)

def match(pattern, string, flags=0):
    return compile(pattern, flags).match(string)

def fullmatch(pattern, string, flags=0):
    return compile(pattern, flags).fullmatch(string)

def findall(pattern, string, flags=0):
    return compile(pattern, flags).findall(string)

def finditer(pattern, string, flags=0):
    return compile(pattern, flags).finditer(string)

def sub(pattern, repl, string, count=0, flags=0):
    return compile(pattern, flags).sub(repl, string, count)

def subn(pattern, repl, string, count=0, flags=0):
    return compile(pattern, flags).subn(repl, string, count)

def split(pattern, string, maxsplit=0, flags=0):
    return compile(pattern, flags).split(string, maxsplit)

def escape(pattern):
    return "".join("\\" + c if c in _SPECIAL else c for c in pattern)
