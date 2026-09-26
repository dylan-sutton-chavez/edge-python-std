from test import *

@test("raises accepts a tuple of exceptions")
def accepts_a_tuple():
    with raises((ValueError, ZeroDivisionError)):
        1 / 0

@test("raises lets a different exception through")
def lets_others_through():
    with raises(ZeroDivisionError):
        with raises(ValueError):
            1 / 0

@test("raises names every expected exception when nothing is raised")
def names_a_tuple():
    message = ""
    try:
        with raises((ValueError, KeyError)):
            pass
    except AssertionError as e:
        message = str(e)
    assert message == "Expected ValueError or KeyError, nothing was raised"

@test("raises passes when the message holds match")
def holds_match():
    with raises(ValueError, match="must be positive"):
        raise ValueError("n must be positive")

@test("raises fails when the message lacks match")
def lacks_match():
    message = ""
    try:
        with raises(ValueError, match="must be positive"):
            raise ValueError("too big")
    except AssertionError as e:
        message = str(e)
    assert message == "Expected 'must be positive' in the message, got 'too big'"

@fixture
def items():
    return []

@test("a fixture is injected by keyword", "items")
def injected(items):
    items.append(1)
    assert items == [1]

@test("a fixture is built fresh for every test", "items")
def fresh(items):
    assert items == []

run()
