from _struct import calcsize as _calcsize, pack as _pack, unpack as _unpack

# A bad format or value raises ValueError, the one exception class a program can catch it as.
error = ValueError

def calcsize(format):
    return _calcsize(format)

def pack(format, *values):
    return _pack(format, *values)

def unpack(format, buffer):
    return tuple(_unpack(format, buffer))

def unpack_from(format, buffer, offset=0):
    size = calcsize(format)
    start = offset + len(buffer) if offset < 0 else offset
    if start < 0:
        raise error(f"offset {offset} out of range for {len(buffer)}-byte buffer")
    if len(buffer) - start < size:
        if offset < 0:
            raise error(f"not enough data to unpack {size} bytes at offset {offset}")
        raise error(f"unpack_from requires a buffer of at least {size + offset} bytes for unpacking {size} bytes at offset {offset} (actual buffer size is {len(buffer)})")
    return unpack(format, buffer[start:start + size])

def iter_unpack(format, buffer):
    size = calcsize(format)
    if size == 0:
        raise error("cannot iteratively unpack with a struct of length 0")
    if len(buffer) % size:
        raise error(f"iterative unpacking requires a buffer of a multiple of {size} bytes")
    return iter([unpack(format, buffer[at:at + size]) for at in range(0, len(buffer), size)])

class Struct:
    def __init__(self, format):
        self.format = format
        self.size = calcsize(format)

    def pack(self, *values):
        return pack(self.format, *values)

    def unpack(self, buffer):
        return unpack(self.format, buffer)

    def unpack_from(self, buffer, offset=0):
        return unpack_from(self.format, buffer, offset)

    def iter_unpack(self, buffer):
        return iter_unpack(self.format, buffer)
