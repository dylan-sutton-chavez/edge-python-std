from _math import *
from _math import ceil as _ceil, floor as _floor, trunc as _trunc

pi = 3.141592653589793
e = 2.718281828459045
tau = 6.283185307179586
inf = float("inf")
nan = float("nan")

# The engine turns the rounded value into an int, so the errors are the ones int() raises.
def floor(x):
    return int(_floor(x))

def ceil(x):
    return int(_ceil(x))

def trunc(x):
    return int(_trunc(x))

# Multiplies any values, so a product of ints stays an exact int.
def prod(iterable, *, start=1):
    for value in iterable:
        start *= value
    return start
