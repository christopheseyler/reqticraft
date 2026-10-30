"""Hôte des scripts : lit une requête JSON-RPC ``check`` sur l'entrée standard,
exécute les scripts et renvoie les problèmes sur la sortie standard.
"""

from __future__ import annotations

import json
import os
import runpy
import sys
import traceback
from typing import Any, Iterable

from . import STRUCTURAL, REQUIREMENTS, Check, Document, Element, Problem, Project, _checks


def _where(exc: BaseException, scripts: list[str]) -> str:
    """« regles.py, ligne 12 : » pour la dernière ligne exécutée d'un script."""
    wanted = {os.path.normcase(os.path.abspath(s)) for s in scripts}
    for frame in reversed(traceback.extract_tb(exc.__traceback__)):
        if os.path.normcase(os.path.abspath(frame.filename)) in wanted:
            return f"{os.path.basename(frame.filename)}, ligne {frame.lineno} : "
    return ""


def _describe(exc: BaseException, scripts: list[str]) -> str:
    return f"{_where(exc, scripts)}{type(exc).__name__}: {exc}"


def _ref(target: Any) -> tuple:
    if isinstance(target, Element):
        return target.document.index, target.index
    if isinstance(target, Document):
        return target.index, None
    return None, None


def _targets(project: Project, c: Check) -> Iterable[Any]:
    if c.on == "project":
        return [project]
    docs = [d for d in project.documents if c.doc_types is None or d.type in c.doc_types]
    if c.on == "document":
        return docs
    kinds = REQUIREMENTS if c.on == "requirement" else STRUCTURAL
    return [
        e
        for d in docs
        for e in d.elements
        if e.kind in kinds and (c.types is None or e.type in c.types)
    ]


def _results(c: Check, target: Any, value: Any) -> list[dict]:
    if value is None or value is False:
        return []
    if value is True:
        value = c.message or c.name
    items = [value] if isinstance(value, (str, Problem)) else list(value)
    out = []
    for item in items:
        if isinstance(item, Problem):
            text, at, severity = item.text, item.target or target, item.severity or c.severity
        else:
            text, at, severity = str(item), target, c.severity
        if not text:
            continue
        document, element = _ref(at)
        out.append({"rule": c.name, "severity": severity, "text": text, "document": document, "element": element})
    return out


def run(params: dict) -> dict:
    scripts: list[str] = params.get("scripts", [])
    errors: list[dict] = []
    for path in scripts:
        sys.path.insert(0, os.path.dirname(os.path.abspath(path)))
        try:
            runpy.run_path(path, run_name="__reqticraft__")
        except BaseException as exc:  # noqa: BLE001 - le script ne doit pas arrêter l'hôte
            errors.append({"text": _describe(exc, scripts), "document": None, "element": None})
    project = Project(params)
    problems: list[dict] = []
    for c in _checks:
        for target in _targets(project, c):
            try:
                problems.extend(_results(c, target, c.function(target)))
            except Exception as exc:  # noqa: BLE001
                document, element = _ref(target)
                errors.append(
                    {"text": f"vérification {c.name} : {_describe(exc, scripts)}", "document": document, "element": element}
                )
                break
    return {"checks": len(_checks), "problems": problems, "errors": errors}


def main() -> None:
    out = sys.stdout
    # Les print() des scripts ne doivent pas se mêler à la réponse.
    sys.stdout = sys.stderr
    line = sys.stdin.readline()
    msg = json.loads(line)
    reply: dict = {"jsonrpc": "2.0", "id": msg.get("id")}
    if msg.get("method") != "check":
        reply["error"] = {"code": -32601, "message": f"méthode inconnue : {msg.get('method')}"}
    else:
        try:
            reply["result"] = run(msg.get("params", {}))
        except Exception as exc:  # noqa: BLE001
            reply["error"] = {"code": -32000, "message": f"{type(exc).__name__}: {exc}"}
    out.write(json.dumps(reply, ensure_ascii=False) + "\n")
    out.flush()


if __name__ == "__main__":
    main()
