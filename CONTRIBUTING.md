# Contributing to the standard library

Thanks for taking the time to contribute. This guide covers issues, pull requests, and how to build and test a package.

## Issues

Include a minimal failing script, the command used to run it, and what Python's own module does with it. Security reports go to [c.sutton.dylan@gmail.com](mailto:c.sutton.dylan@gmail.com) instead of a public issue.

## Pull Requests

For a new package or a large change, open an issue or email [c.sutton.dylan@gmail.com](mailto:c.sutton.dylan@gmail.com) first so it can be accepted once ready.

- A package that shares its name with a Python module behaves the way that module does, down to its errors and messages.
- New behavior comes with tests, and they pass under Python too, except a test of a limit only Edge has, like a backtracking budget.
- Docs describe the package as it is after the change, and every example prints its output block.

## Style

Comments are one line, at most one per block, and deleted when redundant. No file-header comment or docstring. Comments and docs use no colons, semicolons, or em-dashes.

## Building and Testing

Run these from the repo root before sending. `make wasm` builds every plugin in `rust/` and puts each beside its package.

```bash
make wasm
cargo test --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --target wasm32-unknown-unknown -- -D warnings
(cd edge/json && edge test)
PYTHONPATH=edge/test/src python3 edge/json/tests/json_test.py # the same tests under CPython
```

A package's `docs/` follow the page rules of the site, which `edge build` checks before it packs the package.
