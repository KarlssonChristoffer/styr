# {{name}}

A JSON API with FastAPI (Python, managed with uv). Started from Hub's "API server" template.

## Build, run and test
- Install: `uv sync`
- Run: `uv run fastapi dev` (http://127.0.0.1:8000, interactive docs at `/docs`)
- Test: `uv run pytest`
- Lint: `uv run ruff check`

## Layout
- `app/main.py`: the FastAPI app and its routes (`GET /health` to begin with).
- `tests/`: pytest tests using FastAPI's `TestClient`.

## Rules
- Run the tests and the linter before calling work done.
- Every new route gets a test next to the existing ones.
- Add dependencies with `uv add <package>` (`uv add --dev` for tools), never by editing `pyproject.toml` by hand.
- Keep secrets in `.env` (it is git-ignored); never commit them.
