import math
from test import test, raises, run

@test("the constants hold Python's values")
def constants():
    assert math.pi == 3.141592653589793 and math.e == 2.718281828459045 and math.tau == 2 * math.pi
    assert math.inf > 1e308 and -math.inf < -1e308 and math.nan != math.nan

@test("results a float holds exactly come out exact")
def exact():
    assert math.sqrt(16) == 4.0 and math.sqrt(2) == 1.4142135623730951 and math.exp(0) == 1.0 and math.log(1) == 0.0
    assert math.log2(8) == 3.0 and math.pow(2, 10) == 1024.0 and math.fabs(-3.5) == 3.5 and math.copysign(3.0, -0.0) == -3.0
    assert math.sin(0) == 0.0 and math.cos(0) == 1.0 and math.atan2(0, 1) == 0.0 and math.degrees(math.pi) == 180.0 and math.radians(180) == math.pi
    assert math.fmod(-7, 3) == -1.0 and math.remainder(7, 4) == -1.0 and math.ldexp(0.75, 4) == 12.0 and math.sqrt(True) == 1.0

@test("the other functions agree with Python within a few units in the last place")
def close():
    for got, want in [(math.cbrt(27), 3.0), (math.log10(1000), 3.0), (math.log(8, 2), 3.0), (math.exp(1), math.e), (math.tan(math.pi / 4), 1.0), (math.gamma(5.5), 52.34277778455352), (math.lgamma(10), math.log(362880)), (math.erf(1), 0.8427007929497148), (math.erfc(1) + math.erf(1), 1.0), (math.asinh(math.sinh(1.5)), 1.5), (math.acosh(math.cosh(2)), 2.0), (math.atanh(math.tanh(0.5)), 0.5), (math.expm1(1e-10), 1.00000000005e-10), (math.log1p(1e-10), 9.9999999995e-11), (math.exp2(0.5), math.sqrt(2))]:
        assert math.isclose(got, want, rel_tol=1e-14), (got, want)

@test("a result outside the reals is a domain error, and one too large overflows")
def errors():
    for f, args in [(math.sqrt, [-1]), (math.log, [0]), (math.log, [-1]), (math.acos, [2]), (math.atanh, [1]), (math.acosh, [0.5]), (math.log1p, [-1]), (math.sin, [math.inf]), (math.gamma, [0]), (math.gamma, [-2]), (math.lgamma, [-1]), (math.fmod, [1, 0]), (math.remainder, [math.inf, 1]), (math.pow, [0, -1]), (math.pow, [-8, 1 / 3]), (math.log, [8, 0])]:
        with raises(ValueError, match="math domain error"):
            f(*args)
    for f, args in [(math.exp, [1000]), (math.cosh, [1000]), (math.sinh, [-1000]), (math.exp2, [1024]), (math.expm1, [710]), (math.pow, [1e308, 2]), (math.ldexp, [1.0, 2000]), (math.gamma, [200]), (math.lgamma, [1e308])]:
        with raises(OverflowError, match="math range error"):
            f(*args)
    with raises(ZeroDivisionError, match="float division by zero"):
        math.log(2, 1)

@test("infinities and NaN pass through as they do in Python")
def specials():
    assert math.pow(math.nan, 0) == 1.0 and math.pow(1, math.nan) == 1.0 and math.pow(math.inf, -1) == 0.0 and math.pow(-math.inf, 3) == -math.inf
    assert math.exp(-math.inf) == 0.0 and math.log(math.inf) == math.inf and math.gamma(math.inf) == math.inf and math.lgamma(-math.inf) == math.inf
    assert math.isnan(math.sqrt(math.nan)) and math.atan(math.inf) == math.pi / 2 and math.ldexp(math.inf, -5) == math.inf
    assert math.isinf(-math.inf) and math.isnan(math.nan) and math.isfinite(1e308) and not math.isfinite(math.nan)

@test("floor, ceil and trunc give ints, and an int passes through")
def rounding():
    assert (math.floor(2.7), math.ceil(2.1), math.trunc(-2.7), math.floor(-2.1), math.ceil(-0.5)) == (2, 3, -2, -3, 0)
    assert math.floor(7) == 7 and math.ceil(True) == 1 and type(math.floor(2.5)) is int and math.floor(-1e20) == -100000000000000000000
    with raises(OverflowError, match="cannot convert float infinity to integer"):
        math.floor(math.inf)
    with raises(ValueError, match="cannot convert float NaN to integer"):
        math.ceil(math.nan)

@test("modf and frexp split a float into a tuple")
def parts():
    assert math.modf(2.5) == (0.5, 2.0) and math.modf(-3.25) == (-0.25, -3.0) and math.modf(math.inf) == (0.0, math.inf)
    assert math.frexp(8) == (0.5, 4) and math.frexp(0.0) == (0.0, 0) and math.frexp(-3.0) == (-0.75, 2) and math.frexp(math.inf) == (math.inf, 0)
    assert str(math.modf(-0.0)) == "(-0.0, -0.0)" and type(math.frexp(1.0)) is tuple

@test("the integer functions stay exact")
def integers():
    assert (math.factorial(0), math.factorial(5), math.factorial(33), math.factorial(True)) == (1, 120, 8683317618811886495518194401280000000, 1)
    assert (math.gcd(), math.gcd(12, 18, 24), math.gcd(-4, 6), math.lcm(), math.lcm(4, 6), math.lcm(0, 5)) == (0, 6, 2, 1, 12, 0)
    assert (math.isqrt(0), math.isqrt(17), math.isqrt(2 ** 126)) == (0, 4, 2 ** 63)
    assert (math.comb(5, 2), math.comb(5, 7), math.comb(60, 30), math.comb(13128212440888706149, 2)) == (10, 0, 118264581564861424, 86174980946552499914637086649190852026)
    assert (math.perm(5, 2), math.perm(5), math.perm(5, None), math.perm(3, 4)) == (20, 120, 120, 0)

@test("the integer functions refuse what Python refuses")
def refusals():
    for f, args, message in [
        (math.factorial, [-1], "factorial() not defined for negative values"),
        (math.perm, [-1], "factorial() not defined for negative values"),
        (math.isqrt, [-1], "isqrt() argument must be nonnegative"),
        (math.comb, [-1, 2], "n must be a non-negative integer"),
        (math.comb, [5, -1], "k must be a non-negative integer"),
        (math.perm, [-1, 2], "n must be a non-negative integer"),
    ]:
        with raises(ValueError, match=message):
            f(*args)
    for f, args in [(math.factorial, [2.5]), (math.gcd, [1.5, 2]), (math.comb, [5.0, 2]), (math.isqrt, [4.0])]:
        with raises(TypeError, match="'float' object cannot be interpreted as an integer"):
            f(*args)
    for f, args in [(math.sqrt, ["4"]), (math.hypot, [3, "4"]), (math.fsum, [[1, "2"]]), (math.floor, [None])]:
        with raises(TypeError, match="must be real number, not"):
            f(*args)

@test("fsum adds exactly and rounds once")
def sums():
    assert math.fsum([0.1, 0.2, 0.3]) == 0.6 and 0.1 + 0.2 + 0.3 != 0.6 and math.fsum([1e-16, 1, 1e16]) == 1.0000000000000002e16
    assert math.fsum([1e100, 1.0, -1e100, 1e-100, 1e50, -1.0, -1e50]) == 1e-100 and math.fsum([1, 1e100, 1, -1e100] * 1000) == 2000.0
    assert math.fsum(x / 10 for x in range(10)) == 4.5 and math.fsum(()) == 0.0 and math.fsum([math.inf, 1]) == math.inf and math.isnan(math.fsum([math.nan, 1]))
    with raises(ValueError, match="-inf + inf in fsum"):
        math.fsum([math.inf, -math.inf])
    with raises(OverflowError, match="intermediate overflow in fsum"):
        math.fsum([1e308, 1e308])

@test("prod multiplies any values, so ints stay exact")
def products():
    assert math.prod([1, 2, 3, 4]) == 24 and type(math.prod([2, 3])) is int and math.prod([]) == 1 and math.prod([2.0, 0.5]) == 1.0
    assert math.prod(range(1, 6), start=10) == 1200 and math.prod([2 ** 60, 2 ** 60]) == 2 ** 120

@test("hypot and dist measure as Python does, to the last bit")
def norms():
    assert (math.hypot(), math.hypot(-3), math.hypot(3, 4), math.hypot(3, 4, 12)) == (0.0, 3.0, 5.0, 13.0)
    assert math.hypot(2, 3) == 3.605551275463989 and math.hypot(1e308, 1e308) == 1.4142135623730951e308 and math.hypot(5e-324, 5e-324) == 5e-324
    assert math.hypot(math.inf, math.nan) == math.inf and math.isnan(math.hypot(1, math.nan))
    assert math.dist((0, 0), (3, 4)) == 5.0 and math.dist([1.5, 2.5, 3.5], [0.1, 0.2, 0.3]) == 4.182104733265297 and math.dist(iter([1]), (4,)) == 3.0
    with raises(ValueError, match="both points must have the same number of dimensions"):
        math.dist([1], [1, 2])

@test("isclose takes a relative or an absolute tolerance")
def closeness():
    assert math.isclose(1.0, 1.0 + 1e-10) and not math.isclose(1.0, 1.1) and math.isclose(1.0, 1.1, rel_tol=0.1)
    assert not math.isclose(0.0, 1e-10) and math.isclose(0.0, 1e-10, abs_tol=1e-9) and math.isclose(math.inf, math.inf) and not math.isclose(math.inf, -math.inf)
    with raises(ValueError, match="tolerances must be non-negative"):
        math.isclose(1, 1, rel_tol=-1)

run()
