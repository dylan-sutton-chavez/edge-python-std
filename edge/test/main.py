# `_tests` is what `edge test` reads to drive a file that never calls run().
from .src.test import fixture, test, raises, run, _tests
