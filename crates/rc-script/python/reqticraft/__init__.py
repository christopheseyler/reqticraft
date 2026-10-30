"""API des scripts ReqtiCraft.

Un script enregistre des vérifications avec le décorateur ``@check`` ; ReqtiCraft
les applique au modèle analysé et affiche les problèmes renvoyés. Voir
``docs/python-scripting.md``.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Iterator, Optional, Union

__all__ = ["check", "Problem", "Element", "Document", "Project"]

SEVERITIES = ("error", "warning", "information")
SCOPES = ("requirement", "element", "document", "project")
STRUCTURAL = ("section", "macro_requirement", "requirement", "entity")
REQUIREMENTS = ("macro_requirement", "requirement")


@dataclass
class Check:
    name: str
    function: Callable[[Any], Any]
    severity: str
    on: str
    types: Optional[tuple]
    doc_types: Optional[tuple]
    message: Optional[str]


# Vérifications enregistrées par les scripts.
_checks: list[Check] = []


def _names(v: Union[str, list, tuple, None]) -> Optional[tuple]:
    if v is None:
        return None
    return (v,) if isinstance(v, str) else tuple(v)


def check(
    function: Optional[Callable] = None,
    *,
    name: Optional[str] = None,
    severity: str = "warning",
    on: str = "requirement",
    types: Union[str, list, None] = None,
    doc_types: Union[str, list, None] = None,
    message: Optional[str] = None,
):
    """Enregistre une vérification ; utilisable avec ou sans arguments."""
    if severity not in SEVERITIES:
        raise ValueError(f"sévérité inconnue : {severity!r} (attendu : {', '.join(SEVERITIES)})")
    if on not in SCOPES:
        raise ValueError(f"portée inconnue : {on!r} (attendu : {', '.join(SCOPES)})")

    def register(f: Callable) -> Callable:
        _checks.append(Check(name or f.__name__, f, severity, on, _names(types), _names(doc_types), message))
        return f

    return register(function) if function is not None else register


@dataclass
class Problem:
    """Problème renvoyé par une vérification, pour viser une autre cible ou changer la sévérité."""

    text: str
    target: Union["Element", "Document", None] = None
    severity: Optional[str] = None


def _select(targets: list, doc_type: Optional[str], document: Optional[str]) -> list:
    def keep(t) -> bool:
        d = t if isinstance(t, Document) else t.document
        return (doc_type is None or d.type == doc_type) and (document is None or d.name == document)

    return [t for t in targets if keep(t)]


class Element:
    """Élément capturé (section, exigence, entité), en lecture seule."""

    __slots__ = ("document", "index", "type", "kind", "id", "label", "text", "attributes", "line", "status",
                 "_parent", "_children", "_covers", "_covered_by")

    def __init__(self, document: "Document", index: int, raw: dict):
        self.document = document
        self.index = index
        self.type: str = raw["type"]
        self.kind: str = raw["kind"]
        self.id: Optional[str] = raw.get("id")
        self.label: Optional[str] = raw.get("label")
        self.text: str = raw.get("text") or ""
        self.attributes: dict = {k: v for k, v in raw.get("attributes", [])}
        self.line: int = raw.get("line", 0)
        self.status: str = raw.get("status", "none")
        self._parent: Optional[int] = raw.get("parent")
        self._children: list[int] = raw.get("children", [])
        self._covers: list = []
        self._covered_by: list = []

    @property
    def name(self) -> str:
        if self.id and self.label and self.id != self.label:
            return f"{self.id} {self.label}"
        return self.id or self.label or self.type

    @property
    def is_requirement(self) -> bool:
        return self.kind in REQUIREMENTS

    @property
    def parent(self) -> Optional["Element"]:
        return None if self._parent is None else self.document.elements[self._parent]

    @property
    def children(self) -> list["Element"]:
        return [self.document.elements[i] for i in self._children]

    def attr(self, name: str, default: Any = None) -> Any:
        return self.attributes.get(name, default)

    def ancestors(self) -> Iterator["Element"]:
        p = self.parent
        while p is not None:
            yield p
            p = p.parent

    def descendants(self) -> Iterator["Element"]:
        stack = list(reversed(self.children))
        while stack:
            e = stack.pop()
            yield e
            stack.extend(reversed(e.children))

    def covers(self, doc_type: Optional[str] = None, document: Optional[str] = None) -> list:
        """Éléments (ou documents entiers) couverts par cet élément."""
        return _select(self._covers, doc_type, document)

    def covered_by(self, doc_type: Optional[str] = None, document: Optional[str] = None) -> list:
        """Éléments (ou documents entiers) qui couvrent cet élément."""
        return _select(self._covered_by, doc_type, document)

    def __repr__(self) -> str:
        return f"<Element {self.name!r} de {self.document.name!r}>"


class Document:
    """Document analysé, en lecture seule."""

    def __init__(self, project: "Project", index: int, raw: dict):
        self.project = project
        self.index = index
        self.name: str = raw["name"]
        self.type: str = raw["type"]
        self.text: str = raw.get("text") or ""
        self.attributes: dict = {k: v for k, v in raw.get("attributes", [])}
        self.elements: list[Element] = [Element(self, i, e) for i, e in enumerate(raw.get("elements", []))]
        self._roots: list[int] = raw.get("roots", [])
        self._covers: list = []
        self._covered_by: list = []

    @property
    def roots(self) -> list[Element]:
        return [self.elements[i] for i in self._roots]

    @property
    def requirements(self) -> list[Element]:
        return [e for e in self.elements if e.is_requirement]

    def find(self, ident: str) -> Optional[Element]:
        """Élément d'identifiant donné (casse ignorée)."""
        low = ident.lower()
        return next((e for e in self.elements if e.id and e.id.lower() == low), None)

    def covers(self, doc_type: Optional[str] = None, document: Optional[str] = None) -> list:
        return _select(self._covers, doc_type, document)

    def covered_by(self, doc_type: Optional[str] = None, document: Optional[str] = None) -> list:
        return _select(self._covered_by, doc_type, document)

    def __repr__(self) -> str:
        return f"<Document {self.name!r} ({self.type})>"


class Project:
    """Projet analysé, en lecture seule."""

    def __init__(self, raw: dict):
        info = raw.get("project", {})
        self.name: str = info.get("name", "")
        self.dir: str = info.get("dir", "")
        self.documents: list[Document] = [Document(self, i, d) for i, d in enumerate(raw.get("documents", []))]
        for fd, fe, td, te in raw.get("links", []):
            src, dst = self._target(fd, fe), self._target(td, te)
            if dst not in src._covers:
                src._covers.append(dst)
            if src not in dst._covered_by:
                dst._covered_by.append(src)

    def _target(self, d: int, e: Optional[int]) -> Union[Element, Document]:
        doc = self.documents[d]
        return doc if e is None else doc.elements[e]

    def document(self, name: str) -> Optional[Document]:
        return next((d for d in self.documents if d.name == name), None)

    def elements(self) -> Iterator[Element]:
        for d in self.documents:
            yield from d.elements

    def __repr__(self) -> str:
        return f"<Project {self.name!r}>"
