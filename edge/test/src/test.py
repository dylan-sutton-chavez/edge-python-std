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

def _run_one(description: str, func, uses: tuple[str, ...]) -> bool:
    try:
        func(**_build_kwargs(uses))
        print(f"pass. {description}")
        return True
    except AssertionError as e: # test failed
        print(f"fail. {description} (AssertionError: {e})")
    except SystemExit as e: # a test that exits ends only itself, never the run
        print(f"error. {description} (SystemExit: {e})")
    except Exception as e: # unexpected error in the test
        print(f"error. {description} ({type(e).__name__}: {e})")
    return False

def _verdict(passed, failed, total: int) -> int:
    # Only counts that add up to every test pass, so a broken counter or an empty run never reads as green.
    if total == 0:
        return 3
    return 0 if failed == 0 and passed == total else 1

def run() -> None:
    passed = 0
    failed = 0
    for description, func, uses in _tests:
        if _run_one(description, func, uses):
            passed += 1
        else:
            failed += 1
    print(f"{passed} passed, {failed} failed")
    raise SystemExit(_verdict(passed, failed, len(_tests)))
