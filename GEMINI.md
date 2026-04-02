# GEMINI.md

## General Instructions

- When generating new Python code, follow the existing coding style, write
  tests, ensure code coverage is near 100%, and fix any issues after running
  `uv run prek run --all-files`.

## Coding Style

- Use 4 spaces for indentation.
- All functions should have a docstring.
- Follow the
  [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html)
- Use types and be as explicit as possible.
- Refrain from using comments to explain the obvious.

## Regarding Dependencies

- Avoid introducing new external dependencies unless absolutely necessary.
- If a new dependency is required, state the reason.

## Setup Commands

- Install Python: `mise install`
- Install the dependencies: `uv venv` and then `uv sync --all-extras`
- Run for the pre-commit hooks: `uv run prek run --all-files`

## Testing

- Run the tests: `uv run pytest --cov --cov-fail-under=80`

## Type Checking, Formatting, and Linting

- Use `ty` for type checking: `uv run ty check`
- Use `ruff` for formatting: `uv run ruff format`
- Use `ruff` for linting: `uv run ruff check`
