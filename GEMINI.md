# GEMINI.md

## General Instructions

- When generating new Python code, please follow the existing coding style.

## Coding Style

- Use 4 spaces for indentation.
- Follow the
  [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html)
- Use types and be as explicit as possible.
- Refrain from using comments to explain the obvious.

## Regarding Dependencies

- Avoid introducing new external dependencies unless absolutely necessary.
- If a new dependency is required, please state the reason.

## Setup Commands

- Install Python: `mise install`
- Install the dependencies: `uv venv` and then `uv sync --all-extras`
- Run the pre-commit hooks: `uv run pre-commit run --all-files`
- Run the tests: `uv run pytest`

## Formatting and Linting

- Use `ruff` for formatting. Run: `uv run ruff format --all-files`
- Use `ruff` for linting. Run: `uv run ruff check --all-files`

## Testing Instructions

- Run tests: `uv run pytest`
- Find the CI plan in the `.github/workflows/` folder.
- Any commit should pass all the tests, linting, and formatting checks by way of
  the `pre-commit` hooks before you merge.
- Fix any test or type errors until the whole suite is green.
- Add or update tests for the code you change, even if nobody asked.
