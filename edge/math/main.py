from _math import *

pi = 3.141592653589793
e = 2.718281828459045
tau = 6.283185307179586
inf = float("inf")
nan = float("nan")

# Multiplies any values, so a product of ints stays an exact int.
def prod(iterable, *, start=1):
    for value in iterable:
        start *= value
    return start
