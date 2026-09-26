_tests: list[tuple[str, object, tuple[str, ...]]] = []
_fixtures: dict[str, object] = {}

def fixture(func):
    _fixtures[func.__name__] = func
    return func

def test(description: str, *uses: str):
    # Registers a test, injecting `uses` fixtures by keyword.
    def decorator(func):
        _tests.append((description, func, uses))
        return func
    return decorator

class Raises:
    def __init__(self, exc_type: type, match: str | None) -> None:
        self.exc_type = exc_type
        self.match = match

    def __enter__(self):
        return self

    def __exit__(self, etype, exc, _tb) -> bool:
        if etype is None:
            types = self.exc_type if isinstance(self.exc_type, tuple) else (self.exc_type,)
            names = " or ".join(t.__name__ for t in types)
            raise AssertionError(f"Expected {names}, nothing was raised")
        # Swallow the exception only if it matches the expected type.
        if not issubclass(etype, self.exc_type):
            return False
        if self.match is not None and self.match not in str(exc):
            raise AssertionError(f"Expected {self.match!r} in the message, got {str(exc)!r}")
        return True

def raises(exc_type: type, match: str | None = None) -> Raises:
    return Raises(exc_type, match)

def _build_kwargs(uses: tuple[str, ...]) -> dict:
    kwargs = {}
    for name in uses:
        if name not in _fixtures:
            raise KeyError(f"Unknown fixture: {name!r}")
        kwargs[name] = _fixtures[name]() # call fixture fresh per test
    return kwargs

def run() -> None:
    passed = 0
    failed = 0
    for description, func, uses in _tests:
        try:
            func(**_build_kwargs(uses))
            print(f"pass. {description}")
            passed += 1
        except AssertionError as e: # test failed
            print(f"fail. {description} (AssertionError: {e})")
            failed += 1
        except Exception as e: # unexpected error in the test
            print(f"error. {description} ({type(e).__name__}: {e})")
            failed += 1
    print(f"{passed} passed, {failed} failed")
    raise SystemExit(1 if failed else 0)
