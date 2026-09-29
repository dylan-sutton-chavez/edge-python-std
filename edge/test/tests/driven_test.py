from test import test

# No run() at the end, `edge test` drives whatever a file registers.
@test("a file that never calls run() is driven by edge test")
def driven():
    assert 2 + 2 == 4
