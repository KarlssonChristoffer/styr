# {{name}}

A Python project managed with uv. Started from Hub's "Python" template.

## Build, run and test
- Install: `uv sync`
- Run: `uv run python -m app` (optionally with a name: `uv run python -m app Ada`)
- Test: `uv run pytest`
- Lint: `uv run ruff check`

## Layout
- `app/__init__.py`: the code (`greet` to begin with).
- `app/__main__.py`: the entry point for `python -m app`.
- `tests/`: pytest tests.

## Rules
- Run the tests and the linter before calling work done.
- Write a test with every new behaviour.
- Add dependencies with `uv add <package>` (`uv add --dev` for tools), never by editing `pyproject.toml` by hand.
