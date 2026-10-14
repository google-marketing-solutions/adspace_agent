"""Persistent Python REPL kernel used by the AdSpace Agent `python_repl_ast` tool.

Reads one JSON object per line from stdin (`{"code": "..."}`), executes the
code with the same semantics as LangChain's ``PythonAstREPLTool`` (all
statements are executed; if the last statement is an expression its value is
returned), and writes one JSON object per line to stdout.
"""

import ast
import contextlib
import io
import json
import sys
import traceback

_GLOBALS = {"__name__": "__main__"}


def _run(code: str) -> dict:
    buffer = io.StringIO()
    result = None
    try:
        tree = ast.parse(code)
        with contextlib.redirect_stdout(buffer), contextlib.redirect_stderr(buffer):
            if tree.body and isinstance(tree.body[-1], ast.Expr):
                last = tree.body.pop()
                if tree.body:
                    exec(compile(tree, "<python_repl_ast>", "exec"), _GLOBALS)
                value = eval(
                    compile(ast.Expression(last.value), "<python_repl_ast>", "eval"),
                    _GLOBALS,
                )
                if value is not None:
                    result = repr(value)
            else:
                exec(compile(tree, "<python_repl_ast>", "exec"), _GLOBALS)
        return {"ok": True, "stdout": buffer.getvalue(), "result": result}
    except SystemExit:
        return {"ok": False, "stdout": buffer.getvalue(), "error": "SystemExit is not allowed in the REPL."}
    except BaseException:  # noqa: BLE001 - report every failure to the model
        return {"ok": False, "stdout": buffer.getvalue(), "error": traceback.format_exc()}


def main() -> None:
    out = sys.__stdout__
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except json.JSONDecodeError:
            continue
        response = _run(request.get("code", ""))
        out.write(json.dumps(response) + "\n")
        out.flush()


if __name__ == "__main__":
    main()
