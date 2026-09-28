import struct
from test import test, raises, run

@test("every numeric code packs and unpacks back to its value")
def numbers():
    for code, value in [("b", -5), ("B", 250), ("h", -300), ("H", 60000), ("i", -7), ("I", 4000000000), ("l", -9), ("L", 9), ("q", -2 ** 63), ("Q", 2 ** 64 - 1), ("n", -1), ("N", 1), ("?", True), ("e", 1.5), ("f", 0.5), ("d", -2.25)]:
        assert struct.unpack(code, struct.pack(code, value)) == (value,)
    assert struct.unpack("f", struct.pack("f", 1)) == (1.0,) and struct.pack("i", True) == struct.pack("i", 1)

@test("a byte order prefix picks the order and the standard sizes")
def orders():
    assert struct.pack("<h", 1) == b"\x01\x00" and struct.pack(">h", 1) == b"\x00\x01" and struct.pack("!h", 1) == b"\x00\x01"
    assert struct.pack(">i", -1) == b"\xff\xff\xff\xff" and struct.unpack("!I", struct.pack("!I", 70000)) == (70000,)
    assert [struct.calcsize("<" + c) for c in "xcbB?hHiIlLqQefd"] == [1, 1, 1, 1, 1, 2, 2, 4, 4, 4, 4, 8, 8, 2, 4, 8]
    assert struct.calcsize("l") == 8 and struct.calcsize("<l") == 4 and struct.calcsize("P") == 8

@test("a format with no prefix aligns every item, and a zero count aligns the end")
def alignment():
    assert [struct.calcsize(f) for f in ["bi", "ib", "ib0i", "bq", "?h", "c3sh", "xd"]] == [8, 5, 8, 16, 4, 6, 16]
    assert struct.pack("bi", 1, 2) == b"\x01\x00\x00\x00\x02\x00\x00\x00" and struct.calcsize("<bi") == 5

@test("whitespace separates items but never a count from its code")
def spacing():
    assert struct.calcsize("< i h") == 6
    with raises(struct.error, match="bad char in struct format"):
        struct.calcsize("1 i")

@test("s pads or cuts to its count, p leads with a length, c is one byte")
def strings():
    assert struct.pack("3s", b"abcdef") == b"abc" and struct.pack("4s", b"ab") == b"ab\x00\x00" and struct.pack("0s", b"x") == b""
    assert struct.unpack("4s", b"abcd") == (b"abcd",) and struct.unpack("2c", b"ab") == (b"a", b"b")
    assert struct.pack("5p", b"abcdef") == b"\x04abcd" and struct.unpack("5p", struct.pack("5p", b"ab")) == (b"ab",)

@test("? packs whether a value is true, and any nonzero byte unpacks as True")
def truth():
    assert struct.pack("3?", "x", 0, []) == b"\x01\x00\x00" and struct.unpack("?", b"\x02") == (True,)

@test("half precision rounds to even, and a float too large for its code overflows")
def floats():
    assert struct.pack("<e", 1.5) == b"\x00>" and struct.pack("<e", 65504.0) == b"\xff{" and struct.pack("<e", float("inf")) == b"\x00|"
    assert struct.unpack("<e", b"\x00\x3c") == (1.0,)
    with raises(OverflowError, match="float too large to pack with e format"):
        struct.pack("<e", 65520.0)
    with raises(OverflowError, match="float too large to pack with f format"):
        struct.pack("<f", 1e300)

@test("a value out of range, of the wrong kind or in the wrong number is struct.error")
def refusals():
    for fmt, values, message in [
        ("h", [40000], "'h' format requires -32768 <= number <= 32767"),
        ("B", [256], "'B' format requires 0 <= number <= 255"),
        ("Q", [-1], "'Q' format requires 0 <= number <= 18446744073709551615"),
        ("<l", [2 ** 31], "'l' format requires -2147483648 <= number <= 2147483647"),
        ("i", ["a"], "required argument is not an integer"),
        ("f", ["a"], "required argument is not a float"),
        ("c", [b"ab"], "char format requires a bytes object of length 1"),
        ("s", ["a"], "argument for 's' must be a bytes object"),
        ("2i", [1], "pack expected 2 items for packing (got 1)"),
    ]:
        with raises(struct.error, match=message):
            struct.pack(fmt, *values)
    for fmt, message in [("z", "bad char in struct format"), ("3", "repeat count given without format specifier"), ("<n", "bad char in struct format")]:
        with raises(struct.error, match=message):
            struct.calcsize(fmt)
    with raises(struct.error, match="unpack requires a buffer of 4 bytes"):
        struct.unpack("i", b"abc")

@test("unpack_from reads at an offset, and iter_unpack walks a buffer of records")
def offsets():
    assert struct.unpack_from("<h", b"\x00\x01\x00", 1) == (1,) and struct.unpack_from("<h", b"abc", -2) == (25442,)
    for offset, message in [(1, "unpack_from requires a buffer of at least 5 bytes for unpacking 4 bytes at offset 1 (actual buffer size is 3)"), (-1, "not enough data to unpack 4 bytes at offset -1"), (-5, "offset -5 out of range for 3-byte buffer")]:
        with raises(struct.error, match=message):
            struct.unpack_from("<i", b"abc", offset)
    assert list(struct.iter_unpack("<h", b"\x01\x00\x02\x00")) == [(1,), (2,)]
    with raises(struct.error, match="iterative unpacking requires a buffer of a multiple of 2 bytes"):
        struct.iter_unpack("<h", b"abc")

@test("a Struct keeps its format and size and carries every operation")
def compiled():
    s = struct.Struct("<hi")
    assert s.format == "<hi" and s.size == 6
    assert s.unpack(s.pack(1, 2)) == (1, 2) and s.unpack_from(b"\x00" + s.pack(3, 4), 1) == (3, 4)
    assert list(s.iter_unpack(s.pack(5, 6) * 2)) == [(5, 6), (5, 6)]

run()
