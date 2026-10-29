"""Génère le corpus de test maison de ReqtiCraft (dossier fixtures/).

Scénario « thermostat connecté » sur trois niveaux :
spécification (THS_REQ*), conception (DES_REQ*), tests (TST_REQ*).
Les mêmes contenus sont écrits en texte, docx, xlsx et PDF, avec la syntaxe
documentée des types par défaut (identifiant « XXX_REQn : libellé », texte,
références « [Covers: ...] »). Les résultats attendus sont dans expected.json.

Uniquement la bibliothèque standard : python -I tools/gen_fixtures.py
"""

import json
import zipfile
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parent.parent / "fixtures" / "thermostat"

# Chaque bloc : (titre de section, [exigences]). Exigence : (id, libellé, texte, attributs, couvre).
SPEC = [
    ("Mesure", [
        ("THS_REQ1", "Mesure de la température",
         "Le thermostat doit mesurer la température ambiante toutes les 10 secondes.",
         {"Priority": "High"}, []),
        ("THS_REQ2", "Précision de mesure",
         "La précision de mesure doit être meilleure que 0,5 °C entre 5 °C et 35 °C.",
         {"Priority": "High"}, []),
    ]),
    ("Régulation", [
        ("THS_REQ3", "Consigne",
         "L'utilisateur doit pouvoir régler une consigne entre 7 °C et 30 °C.",
         {"Priority": "Medium"}, []),
        ("THS_REQ4", "Hystérésis",
         "Le chauffage doit être commandé avec une hystérésis de 0,3 °C.",
         {"Priority": "Medium"}, []),
        ("THS_REQ5", "Mode absence",
         "Un mode absence doit abaisser la consigne de 3 °C.",
         {"Priority": "Low"}, []),
    ]),
    ("Connectivité", [
        ("THS_REQ6", "Pilotage à distance",
         "La consigne doit pouvoir être modifiée depuis une application mobile.",
         {"Priority": "Low"}, []),
    ]),
]

DESIGN = [
    ("Acquisition", [
        ("DES_REQ1", "Échantillonnage du capteur",
         "Le capteur est lu par interruption toutes les 10 s.",
         {}, ["THS_REQ1"]),
        ("DES_REQ2", "Étalonnage",
         "Une table d'étalonnage en 16 points corrige la non-linéarité du capteur.",
         {}, ["THS_REQ2"]),
    ]),
    ("Régulateur", [
        ("DES_REQ3", "Bornes de consigne",
         "La consigne est saturée entre 7 °C et 30 °C.",
         {}, ["THS_REQ3"]),
        ("DES_REQ4", "Commande tout-ou-rien",
         "Le relais est commandé avec un seuil haut et un seuil bas.",
         {}, ["THS_REQ4"]),
        ("DES_REQ5", "API de consigne",
         "Une API REST expose la lecture et l'écriture de la consigne.",
         {}, ["THS_REQ6", "THS_REQ99"]),
    ]),
    ("Exigences dérivées", [
        ("DES_REQ6", "Chien de garde",
         "Un chien de garde redémarre le microcontrôleur après 2 s sans activité.",
         {}, []),
        ("DES_REQ7", "Journal d'erreurs",
         "Les 100 dernières erreurs sont conservées en mémoire non volatile.",
         {}, []),
    ]),
]

TESTS = [
    ("Tests unitaires", [
        ("TST_REQ1", "Période d'échantillonnage", "Vérifier 6 lectures par minute.", {}, ["DES_REQ1"]),
        ("TST_REQ2", "Table d'étalonnage", "Comparer 16 points à une sonde de référence.", {}, ["DES_REQ2"]),
        ("TST_REQ3", "Saturation", "Envoyer 5 °C et 35 °C, vérifier 7 °C et 30 °C.", {}, ["DES_REQ3"]),
        ("TST_REQ4", "Seuils du relais", "Balayer la température autour de la consigne.", {}, ["DES_REQ4"]),
    ]),
    ("Tests d'intégration", [
        ("TST_REQ5", "API REST", "Lire puis écrire la consigne par l'API.", {}, ["DES_REQ5"]),
        ("TST_REQ6", "Chien de garde", "Bloquer la boucle principale, vérifier le redémarrage.", {}, ["DES_REQ6"]),
    ]),
]

DOCUMENTS = {
    "specification": ("Spécification du thermostat", SPEC),
    "design": ("Conception du thermostat", DESIGN),
    "tests": ("Plan de tests du thermostat", TESTS),
}

EXPECTED = {
    "documents": {"specification": 6, "design": 7, "tests": 6},
    "coverage": [
        {"covered": "specification", "covering": "design", "covered_count": 5, "total": 6},
        {"covered": "design", "covering": "tests", "covered_count": 6, "total": 7},
    ],
    "uncovered": ["THS_REQ5", "DES_REQ7"],
    "derived": ["DES_REQ6", "DES_REQ7"],
    "undefined_references": [{"from": "DES_REQ5", "target": "THS_REQ99"}],
    # Projets partiels (code, json, composite) : couverture attendue par document couvert.
    "partial": {
        "code/thermostat.rqtf": {"Conception": [5, 7]},
        "json/thermostat.rqtf": {"Spécification": [5, 6]},
        "composite/thermostat.rqtf": {"Spécification": [5, 6]},
    },
}


# --- Texte ------------------------------------------------------------------

def text_lines(title, blocks):
    lines = [f"1. {title}", "Document de test ReqtiCraft, créé pour le corpus maison.", ""]
    for i, (section, reqs) in enumerate(blocks, start=2):
        lines.append(f"{i}. {section}")
        for rid, label, body, attrs, covers in reqs:
            lines.append(f"{rid} : {label}")
            lines.append(f"<<{body}>>")
            for name, value in attrs.items():
                lines.append(f"- {name} : {value}")
            if covers:
                lines.append(f"[Covers: {', '.join(covers)}]")
            lines.append("")
    return lines


# --- docx -------------------------------------------------------------------

DOCX_CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"""

DOCX_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"""

DOCX_DOC_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"""

W = 'xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"'


def docx_style(style_id, name, heading_level=None):
    outline = f'<w:pPr><w:outlineLvl w:val="{heading_level - 1}"/></w:pPr>' if heading_level else ""
    return (f'<w:style w:type="paragraph" w:styleId="{style_id}"><w:name w:val="{name}"/>'
            f'<w:basedOn w:val="Normal"/>{outline}</w:style>')


DOCX_STYLES = (
    f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {W}>'
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
    + docx_style("Heading1", "heading 1", 1)
    + docx_style("Heading2", "heading 2", 2)
    + docx_style("RequirementID", "Requirement_ID")
    + docx_style("RequirementText", "Requirement_Text")
    + "</w:styles>"
)


def docx_paragraph(style, text):
    return (f'<w:p><w:pPr><w:pStyle w:val="{style}"/></w:pPr>'
            f'<w:r><w:t xml:space="preserve">{escape(text)}</w:t></w:r></w:p>')


def write_docx(path, title, blocks):
    paragraphs = [docx_paragraph("Heading1", title),
                  docx_paragraph("Normal", "Document de test ReqtiCraft, créé pour le corpus maison.")]
    for section, reqs in blocks:
        paragraphs.append(docx_paragraph("Heading2", section))
        for rid, label, body, attrs, covers in reqs:
            paragraphs.append(docx_paragraph("RequirementID", f"{rid} : {label}"))
            paragraphs.append(docx_paragraph("RequirementText", body))
            for name, value in attrs.items():
                paragraphs.append(docx_paragraph("Normal", f"{name} : {value}"))
            if covers:
                paragraphs.append(docx_paragraph("Normal", f"[Covers: {', '.join(covers)}]"))
    document = (f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {W}><w:body>'
                + "".join(paragraphs) + "</w:body></w:document>")
    write_zip(path, {
        "[Content_Types].xml": DOCX_CONTENT_TYPES,
        "_rels/.rels": DOCX_RELS,
        "word/_rels/document.xml.rels": DOCX_DOC_RELS,
        "word/document.xml": document,
        "word/styles.xml": DOCX_STYLES,
    })


# --- xlsx -------------------------------------------------------------------

XLSX_CONTENT_TYPES = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"""

XLSX_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"""

XLSX_WORKBOOK_RELS = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"""


def column_letter(index):
    return chr(ord("A") + index)


def write_xlsx(path, blocks):
    header = ["ID", "Libellé", "Texte", "Priority", "Owner", "Couverture"]
    rows = [header]
    for _, reqs in blocks:
        for rid, label, body, attrs, covers in reqs:
            cover_text = f"[Covers: {', '.join(covers)}]" if covers else ""
            rows.append([rid, label, body, attrs.get("Priority", ""), "", cover_text])
    xml_rows = []
    for r, values in enumerate(rows, start=1):
        cells = "".join(
            f'<c r="{column_letter(c)}{r}" t="inlineStr"><is><t xml:space="preserve">{escape(v)}</t></is></c>'
            for c, v in enumerate(values) if v
        )
        xml_rows.append(f'<row r="{r}">{cells}</row>')
    sheet = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
             '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
             + "".join(xml_rows) + "</sheetData></worksheet>")
    workbook = ('<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
                '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
                '<sheets><sheet name="Exigences" sheetId="1" r:id="rId1"/></sheets></workbook>')
    write_zip(path, {
        "[Content_Types].xml": XLSX_CONTENT_TYPES,
        "_rels/.rels": XLSX_RELS,
        "xl/workbook.xml": workbook,
        "xl/_rels/workbook.xml.rels": XLSX_WORKBOOK_RELS,
        "xl/worksheets/sheet1.xml": sheet,
    })


# --- PDF --------------------------------------------------------------------

def pdf_string(text):
    raw = text.encode("cp1252")
    return b"(" + raw.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)") + b")"


def write_pdf(path, title, blocks):
    """PDF 1.4 : Helvetica (WinAnsi), une ligne par paragraphe, signets pour les titres."""
    lines = []  # (texte, taille, est_titre)
    lines.append((f"1. {title}", 16, True))
    lines.append(("Document de test ReqtiCraft, créé pour le corpus maison.", 10, False))
    for i, (section, reqs) in enumerate(blocks, start=2):
        lines.append((f"{i}. {section}", 13, True))
        for rid, label, body, attrs, covers in reqs:
            lines.append((f"{rid} : {label}", 10, False))
            lines.append((body, 10, False))
            for name, value in attrs.items():
                lines.append((f"{name} : {value}", 10, False))
            if covers:
                lines.append((f"[Covers: {', '.join(covers)}]", 10, False))

    page_height, top, bottom, left = 842, 800, 50, 56
    pages, current, y = [], [], top
    for text, size, is_title in lines:
        step = size + (10 if is_title else 4)
        if y - step < bottom:
            pages.append(current)
            current, y = [], top
        y -= step
        current.append((text, size, is_title, y))
    pages.append(current)

    objects = {}  # numéro -> contenu (bytes)
    catalog, outlines, pages_obj, font = 1, 2, 3, 4
    next_id = 5
    page_ids, titles = [], []
    for page_lines in pages:
        content_id, page_id = next_id, next_id + 1
        next_id += 2
        stream = b"".join(
            b"BT /F1 %d Tf %d %d Td " % (size, left, y) + pdf_string(text) + b" Tj ET\n"
            for text, size, _, y in page_lines
        )
        objects[content_id] = b"<< /Length %d >>\nstream\n" % len(stream) + stream + b"endstream"
        objects[page_id] = (b"<< /Type /Page /Parent 3 0 R /MediaBox [0 0 595 %d] "
                            b"/Resources << /Font << /F1 4 0 R >> >> /Contents %d 0 R >>"
                            % (page_height, content_id))
        page_ids.append(page_id)
        titles += [(text, page_id, y) for text, _, is_title, y in page_lines if is_title]

    # Signets à plat : un par titre.
    outline_ids = list(range(next_id, next_id + len(titles)))
    for n, ((text, page_id, y), oid) in enumerate(zip(titles, outline_ids)):
        links = b""
        if n > 0:
            links += b" /Prev %d 0 R" % outline_ids[n - 1]
        if n < len(titles) - 1:
            links += b" /Next %d 0 R" % outline_ids[n + 1]
        objects[oid] = (b"<< /Title " + pdf_string(text) + b" /Parent 2 0 R"
                        + links + b" /Dest [%d 0 R /XYZ 0 %d 0] >>" % (page_id, y + 20))
    objects[catalog] = b"<< /Type /Catalog /Pages 3 0 R /Outlines 2 0 R /PageMode /UseOutlines >>"
    objects[outlines] = (b"<< /Type /Outlines /First %d 0 R /Last %d 0 R /Count %d >>"
                         % (outline_ids[0], outline_ids[-1], len(outline_ids)))
    objects[pages_obj] = (b"<< /Type /Pages /Kids [" + b" ".join(b"%d 0 R" % p for p in page_ids)
                          + b"] /Count %d >>" % len(page_ids))
    objects[font] = (b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
                     b"/Encoding /WinAnsiEncoding >>")

    out = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    offsets = {}
    for num in sorted(objects):
        offsets[num] = len(out)
        out += b"%d 0 obj\n" % num + objects[num] + b"\nendobj\n"
    xref = len(out)
    count = max(objects) + 1
    out += b"xref\n0 %d\n0000000000 65535 f \n" % count
    for num in range(1, count):
        out += b"%010d 00000 n \n" % offsets[num]
    out += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (count, xref)
    path.write_bytes(bytes(out))


# --- Projets ----------------------------------------------------------------

NAMES = {"specification": "Spécification", "design": "Conception", "tests": "Tests"}


def write_project(path, docs, covers, extra=()):
    """Projet au format XML. docs : [(nom, type, chemin)] ; covers : [(couvert, couvrant)]."""
    xml = ['<?xml version="1.0" encoding="UTF-8"?>', "<Project>"]
    for x, (name, doc_type, doc_path) in enumerate(docs):
        xml.append(f'<Document name="{name}" positionX="{800 + 700 * x}" '
                   f'positionY="{400 + 600 * x}" type="{doc_type}" path="{doc_path}">')
        xml.append("</Document>")
    xml.extend(extra)
    for covered, covering in covers:
        xml.append(f'<Cover outputDocument="{covered}" positionX="0" positionY="0">')
        xml.append(f'<InputDocument name="{covering}" />')
        xml.append("</Cover>")
    xml.append("</Project>")
    write_text(path, xml)


def write_projects(directory, doc_type, extension):
    docs = [(NAMES[k], doc_type, f"{k}.{extension}") for k in NAMES]
    covers = [(NAMES["specification"], NAMES["design"]), (NAMES["design"], NAMES["tests"])]
    write_project(directory / "thermostat.rqtf", docs, covers)

    ini = ["[Files]", "Names=" + ",".join(NAMES.values()), ""]
    covered_by = {"specification": "design", "design": "tests"}
    for x, key in enumerate(NAMES):
        ini.append(f"[{NAMES[key]}]")
        if key in covered_by:
            ini.append(f"Cover1={NAMES[covered_by[key]]}")
        ini.append(f"GraphicPosition={800 + 700 * x}@{400 + 600 * x}")
        ini.append(f"Type={doc_type}")
        ini.append(f"Path={key}.{extension}")
        ini.append("")
    write_text(directory / "thermostat-ini.rqtf", ini)


# --- Code, JSON, projet composite --------------------------------------------

CODE = {
    "src/acquisition.c": [
        "/* Acquisition de la température. */",
        "#include \"sensor.h\"",
        "",
        "// Implements DES_REQ1",
        "void sample_on_interrupt(void) { /* ... */ }",
        "",
        "/* Covers: DES_REQ2 */",
        "static const float calibration[16] = { 0 };",
    ],
    "src/sensor.h": ["#pragma once", "void sample_on_interrupt(void);"],
    "src/regulator.c": [
        "// Implements DES_REQ3, DES_REQ4",
        "int clamp_setpoint(int value) { return value; }",
    ],
    "src/api/rest.py": [
        "# Covers: DES_REQ5",
        "def put_setpoint(value):",
        "    return value",
    ],
    "src/notes.txt": ["Ce fichier n'est pas du code : // Implements DES_REQ7 doit être ignoré."],
}

JSON_SPEC = {
    "title": "Spécification du thermostat",
    "requirements": [
        {"id": rid, "label": label, "text": body, "priority": attrs.get("Priority")}
        for _, reqs in SPEC
        for rid, label, body, attrs, _ in reqs
    ],
}


def write_extras():
    # Tableur analysé en XML : mêmes documents, type ExcelXML.
    docs = [(NAMES[k], "ExcelXML", f"{k}.xlsx") for k in NAMES]
    covers = [(NAMES["specification"], NAMES["design"]), (NAMES["design"], NAMES["tests"])]
    write_project(ROOT / "xlsx" / "thermostat-xml.rqtf", docs, covers)

    code = ROOT / "code"
    for rel, lines in CODE.items():
        target = code / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        write_text(target, lines)
    write_project(code / "thermostat.rqtf",
                  [(NAMES["design"], "Text", "../text/design.txt"), ("Code", "Code", "src")],
                  [(NAMES["design"], "Code")])

    json_dir = ROOT / "json"
    json_dir.mkdir(parents=True, exist_ok=True)
    (json_dir / "specification.json").write_text(
        json.dumps(JSON_SPEC, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    write_project(json_dir / "thermostat.rqtf",
                  [(NAMES["specification"], "JSON", "specification.json"),
                   (NAMES["design"], "Text", "../text/design.txt")],
                  [(NAMES["specification"], NAMES["design"])])

    # Projet composite : la spécification du projet remplace celle du sous-projet
    # « Fournisseur », dont la conception la couvre.
    composite = ROOT / "composite"
    supplier = composite / "fournisseur"
    supplier.mkdir(parents=True, exist_ok=True)
    write_project(supplier / "fournisseur.rqtf",
                  [(NAMES["specification"], "Text", "../../text/specification.txt"),
                   (NAMES["design"], "Text", "../../text/design.txt")],
                  [(NAMES["specification"], NAMES["design"])])
    spec = NAMES["specification"]
    write_project(composite / "thermostat.rqtf",
                  [(spec, "Text", "../text/specification.txt")],
                  [],
                  extra=[
                      '<ProjectReference name="Fournisseur" positionX="800" positionY="1200" '
                      'type="_ProjectReference" path="fournisseur\\fournisseur.rqtfimage">',
                      "</ProjectReference>",
                      f'<ReplacingPath><Component name="{spec}" />',
                      f'<ReplacedPath><Component name="Fournisseur" /><Component name="{spec}" /></ReplacedPath>',
                      "</ReplacingPath>",
                  ])


# --- Formats P1/P2 : documents complets ---------------------------------------

def doc_items(title, blocks):
    """Suite de (genre, texte) commune aux formats à paragraphes."""
    items = [("h1", f"1. {title}"), ("p", "Document de test ReqtiCraft, créé pour le corpus maison.")]
    for i, (section, reqs) in enumerate(blocks, start=2):
        items.append(("h2", f"{i}. {section}"))
        for rid, label, body, attrs, covers in reqs:
            items.append(("req", f"{rid} : {label}"))
            items.append(("text", body))
            for name, value in attrs.items():
                items.append(("p", f"{name} : {value}"))
            if covers:
                items.append(("p", f"[Covers: {', '.join(covers)}]"))
    return items


def write_markdown(path, title, blocks):
    lines = []
    for kind, text in doc_items(title, blocks):
        prefix = {"h1": "# ", "h2": "## "}.get(kind, "")
        lines += [prefix + text, ""]
    write_text(path, lines)


def write_html(path, title, blocks):
    tags = {"h1": "h1", "h2": "h2"}
    body = "\n".join(f"<{tags.get(k, 'p')}>{escape(t)}</{tags.get(k, 'p')}>"
                     for k, t in doc_items(title, blocks))
    write_text(path, ["<!DOCTYPE html>", '<html lang="fr"><head><meta charset="utf-8">',
                      f"<title>{escape(title)}</title></head><body>", body, "</body></html>"])


def write_asciidoc(path, title, blocks):
    lines = []
    for kind, text in doc_items(title, blocks):
        if kind == "h1":
            lines += ["= " + text.split(". ", 1)[1], ""]
        elif kind == "h2":
            lines += ["== " + text.split(". ", 1)[1], ""]
        elif kind == "text":
            lines += ["____", text, "____", ""]
        else:
            lines += [text, ""]
    write_text(path, lines)


def rtf_escape(text):
    out = []
    for ch in text:
        if ch in "\\{}":
            out.append("\\" + ch)
        elif ord(ch) > 127:
            code = ord(ch) if ord(ch) < 32768 else ord(ch) - 65536
            out.append(f"\\u{code}?")
        else:
            out.append(ch)
    return "".join(out)


def write_rtf(path, title, blocks):
    styles = {"h1": 1, "h2": 2, "req": 3, "text": 4, "p": 0}
    lines = [r"{\rtf1\ansi\ansicpg1252\deff0{\fonttbl{\f0 Arial;}}",
             r"{\stylesheet{\s0 Normal;}{\s1\sbasedon0 heading 1;}{\s2\sbasedon0 heading 2;}"
             r"{\s3\sbasedon0 Requirement_ID;}{\s4\sbasedon0 Requirement_Text;}}"]
    for kind, text in doc_items(title, blocks):
        lines.append(rf"\pard\s{styles[kind]} {rtf_escape(text)}\par")
    lines.append("}")
    write_text(path, lines)


ODF_NS = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
          'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
          'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"')


def write_odt(path, title, blocks):
    names = {"req": "Requirement_5f_ID", "text": "Requirement_5f_Text", "p": "Standard"}
    body = []
    for kind, text in doc_items(title, blocks):
        if kind in ("h1", "h2"):
            level = kind[1]
            body.append(f'<text:h text:style-name="Heading_20_{level}" text:outline-level="{level}">'
                        f"{escape(text)}</text:h>")
        else:
            body.append(f'<text:p text:style-name="{names[kind]}">{escape(text)}</text:p>')
    content = (f'<?xml version="1.0" encoding="UTF-8"?><office:document-content {ODF_NS}>'
               "<office:body><office:text>" + "".join(body) + "</office:text></office:body>"
               "</office:document-content>")
    styles = (f'<?xml version="1.0" encoding="UTF-8"?><office:document-styles {ODF_NS}><office:styles>'
              '<style:style style:name="Standard" style:family="paragraph"/>'
              '<style:style style:name="Heading_20_1" style:display-name="Heading 1" style:family="paragraph"/>'
              '<style:style style:name="Heading_20_2" style:display-name="Heading 2" style:family="paragraph"/>'
              '<style:style style:name="Requirement_5f_ID" style:display-name="Requirement_ID" style:family="paragraph"/>'
              '<style:style style:name="Requirement_5f_Text" style:display-name="Requirement_Text" style:family="paragraph"/>'
              "</office:styles></office:document-styles>")
    write_zip(path, {
        "mimetype": "application/vnd.oasis.opendocument.text",
        "content.xml": content,
        "styles.xml": styles,
        "META-INF/manifest.xml": ('<?xml version="1.0" encoding="UTF-8"?><manifest:manifest '
                                  'xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">'
                                  '<manifest:file-entry manifest:full-path="/" '
                                  'manifest:media-type="application/vnd.oasis.opendocument.text"/>'
                                  "</manifest:manifest>"),
    })


def mif_string(text):
    return (text.replace("\\", "\\\\").replace(">", "\\>").replace("'", "\\q").replace("`", "\\Q"))


def write_mif(path, title, blocks):
    tags = {"h1": "Heading1", "h2": "Heading2", "req": "Requirement_ID", "text": "Requirement_Text", "p": "Body"}
    lines = ["<MIFFile 2019> # Corpus de test ReqtiCraft", "<TextFlow <TFTag `A'>"]
    for kind, text in doc_items(title, blocks):
        lines.append(f" <Para <PgfTag `{tags[kind]}'> <ParaLine <String `{mif_string(text)}'>>>")
    lines.append(">")
    write_text(path, lines)


PPT_NS = ('xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
          'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" '
          'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"')


def write_pptx(path, title, blocks):
    slides = [[title, "Document de test ReqtiCraft, créé pour le corpus maison."]]
    for section, reqs in blocks:
        lines = [section]
        for rid, label, body, attrs, covers in reqs:
            lines += [f"{rid} : {label}", body]
            lines += [f"{n} : {v}" for n, v in attrs.items()]
            if covers:
                lines.append(f"[Covers: {', '.join(covers)}]")
        slides.append(lines)
    parts = {}
    ids, rels = [], []
    for n, lines in enumerate(slides, start=1):
        paras = "".join(f"<a:p><a:r><a:t>{escape(t)}</a:t></a:r></a:p>" for t in lines)
        parts[f"ppt/slides/slide{n}.xml"] = (
            f'<?xml version="1.0" encoding="UTF-8"?><p:sld {PPT_NS}><p:cSld><p:spTree>'
            f"<p:sp><p:txBody>{paras}</p:txBody></p:sp></p:spTree></p:cSld></p:sld>")
        ids.append(f'<p:sldId id="{255 + n}" r:id="rId{n}"/>')
        rels.append(f'<Relationship Id="rId{n}" Type="http://schemas.openxmlformats.org/'
                    f'officeDocument/2006/relationships/slide" Target="slides/slide{n}.xml"/>')
    parts["ppt/presentation.xml"] = (f'<?xml version="1.0" encoding="UTF-8"?><p:presentation {PPT_NS}>'
                                     f"<p:sldIdLst>{''.join(ids)}</p:sldIdLst></p:presentation>")
    parts["ppt/_rels/presentation.xml.rels"] = (
        '<?xml version="1.0" encoding="UTF-8"?><Relationships '
        'xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        + "".join(rels) + "</Relationships>")
    write_zip(path, parts)


def reqif_value(definition, value, kind="STRING"):
    return (f'<ATTRIBUTE-VALUE-{kind} THE-VALUE="{escape(value, {chr(34): "&quot;"})}"><DEFINITION>'
            f"<ATTRIBUTE-DEFINITION-{kind}-REF>{definition}</ATTRIBUTE-DEFINITION-{kind}-REF>"
            f"</DEFINITION></ATTRIBUTE-VALUE-{kind}>")


def write_reqif(path, title, blocks):
    objects, hierarchy, relations, stubs = [], [], [], set()

    def obj(oid, values):
        objects.append(f'<SPEC-OBJECT IDENTIFIER="{oid}" LAST-CHANGE="2026-01-01T00:00:00Z">'
                       "<TYPE><SPEC-OBJECT-TYPE-REF>t-obj</SPEC-OBJECT-TYPE-REF></TYPE>"
                       f"<VALUES>{''.join(values)}</VALUES></SPEC-OBJECT>")

    obj("o-title", [reqif_value("a-chapter", title)])
    sections = []
    for s, (section, reqs) in enumerate(blocks, start=1):
        obj(f"o-s{s}", [reqif_value("a-chapter", section)])
        children = []
        for rid, label, body, attrs, covers in reqs:
            obj(f"o-{rid}", [reqif_value("a-id", rid), reqif_value("a-name", label),
                             reqif_value("a-text", body)]
                + [reqif_value("a-prio", v) for v in attrs.values()])
            children.append(f'<SPEC-HIERARCHY IDENTIFIER="h-{rid}" LAST-CHANGE="2026-01-01T00:00:00Z">'
                            f"<OBJECT><SPEC-OBJECT-REF>o-{rid}</SPEC-OBJECT-REF></OBJECT></SPEC-HIERARCHY>")
            for n, target in enumerate(covers):
                stubs.add(target)
                relations.append(
                    f'<SPEC-RELATION IDENTIFIER="r-{rid}-{n}" LAST-CHANGE="2026-01-01T00:00:00Z">'
                    "<TYPE><SPEC-RELATION-TYPE-REF>t-sat</SPEC-RELATION-TYPE-REF></TYPE>"
                    f"<SOURCE><SPEC-OBJECT-REF>o-{rid}</SPEC-OBJECT-REF></SOURCE>"
                    f"<TARGET><SPEC-OBJECT-REF>x-{target}</SPEC-OBJECT-REF></TARGET></SPEC-RELATION>")
        sections.append(f'<SPEC-HIERARCHY IDENTIFIER="h-s{s}" LAST-CHANGE="2026-01-01T00:00:00Z">'
                        f"<OBJECT><SPEC-OBJECT-REF>o-s{s}</SPEC-OBJECT-REF></OBJECT>"
                        f"<CHILDREN>{''.join(children)}</CHILDREN></SPEC-HIERARCHY>")
    # Objets liés d'une autre spécification : présents pour leur identifiant, hors hiérarchie.
    for target in sorted(stubs):
        objects.append(f'<SPEC-OBJECT IDENTIFIER="x-{target}" LAST-CHANGE="2026-01-01T00:00:00Z">'
                       "<TYPE><SPEC-OBJECT-TYPE-REF>t-obj</SPEC-OBJECT-TYPE-REF></TYPE>"
                       f"<VALUES>{reqif_value('a-id', target)}</VALUES></SPEC-OBJECT>")
    hierarchy.append('<SPEC-HIERARCHY IDENTIFIER="h-title" LAST-CHANGE="2026-01-01T00:00:00Z">'
                     "<OBJECT><SPEC-OBJECT-REF>o-title</SPEC-OBJECT-REF></OBJECT>"
                     f"<CHILDREN>{''.join(sections)}</CHILDREN></SPEC-HIERARCHY>")
    attrs = "".join(
        f'<ATTRIBUTE-DEFINITION-STRING IDENTIFIER="{i}" LONG-NAME="{n}" LAST-CHANGE="2026-01-01T00:00:00Z">'
        "<TYPE><DATATYPE-DEFINITION-STRING-REF>dt-s</DATATYPE-DEFINITION-STRING-REF></TYPE>"
        "</ATTRIBUTE-DEFINITION-STRING>"
        for i, n in [("a-id", "ReqIF.ForeignID"), ("a-name", "ReqIF.Name"), ("a-chapter", "ReqIF.ChapterName"),
                     ("a-text", "ReqIF.Text"), ("a-prio", "Priority")])
    xml = ('<?xml version="1.0" encoding="UTF-8"?>\n'
           '<REQ-IF xmlns="http://www.omg.org/spec/ReqIF/20110401/reqif.xsd">'
           f'<THE-HEADER><REQ-IF-HEADER IDENTIFIER="hdr"><TITLE>{escape(title)}</TITLE>'
           "<REQ-IF-TOOL-ID>gen_fixtures.py</REQ-IF-TOOL-ID></REQ-IF-HEADER></THE-HEADER>"
           "<CORE-CONTENT><REQ-IF-CONTENT><DATATYPES>"
           '<DATATYPE-DEFINITION-STRING IDENTIFIER="dt-s" LONG-NAME="Chaîne" MAX-LENGTH="10000" '
           'LAST-CHANGE="2026-01-01T00:00:00Z"/></DATATYPES><SPEC-TYPES>'
           f'<SPEC-OBJECT-TYPE IDENTIFIER="t-obj" LONG-NAME="Objet" LAST-CHANGE="2026-01-01T00:00:00Z">'
           f"<SPEC-ATTRIBUTES>{attrs}</SPEC-ATTRIBUTES></SPEC-OBJECT-TYPE>"
           '<SPEC-RELATION-TYPE IDENTIFIER="t-sat" LONG-NAME="Satisfies" LAST-CHANGE="2026-01-01T00:00:00Z"/>'
           '<SPECIFICATION-TYPE IDENTIFIER="t-spec" LONG-NAME="Spécification" LAST-CHANGE="2026-01-01T00:00:00Z"/>'
           f"</SPEC-TYPES><SPEC-OBJECTS>{''.join(objects)}</SPEC-OBJECTS>"
           f"<SPEC-RELATIONS>{''.join(relations)}</SPEC-RELATIONS><SPECIFICATIONS>"
           f'<SPECIFICATION IDENTIFIER="spec" LONG-NAME="{escape(title)}" LAST-CHANGE="2026-01-01T00:00:00Z">'
           "<TYPE><SPECIFICATION-TYPE-REF>t-spec</SPECIFICATION-TYPE-REF></TYPE>"
           f"<CHILDREN>{''.join(hierarchy)}</CHILDREN></SPECIFICATION></SPECIFICATIONS>"
           "</REQ-IF-CONTENT></CORE-CONTENT></REQ-IF>")
    write_text(path, [xml])


FULL_FORMATS = {
    # dossier : (type, extension, fonction d'écriture)
    "markdown": ("Markdown", "md", write_markdown),
    "html": ("HTML", "html", write_html),
    "asciidoc": ("AsciiDoc", "adoc", write_asciidoc),
    "rtf": ("RTF", "rtf", write_rtf),
    "odt": ("Writer", "odt", write_odt),
    "mif": ("FrameMaker MIF", "mif", write_mif),
    "pptx": ("PowerPoint", "pptx", write_pptx),
    "reqif": ("ReqIF", "reqif", write_reqif),
}


# --- Formats P1/P2 : sources couvrant la spécification -------------------------
# Chaque source reprend les exigences de conception (DESIGN) et leurs références :
# la spécification doit être couverte à 5/6, avec THS_REQ99 inconnue.

def design_items():
    return [item for _, reqs in DESIGN for item in reqs]


def covers_text(covers):
    return f"[Covers: {', '.join(covers)}]" if covers else ""


def png_with_comment(comment):
    import struct
    import zlib

    def chunk(kind, data):
        return (struct.pack(">I", len(data)) + kind + data
                + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))
    ihdr = struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0)
    raw = zlib.compress(b"\x00" + b"\xff" * 6 + b"\x00" + b"\xff" * 6, 9)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"tEXt", b"Comment\x00" + comment.encode("latin-1"))
            + chunk(b"IDAT", raw) + chunk(b"IEND", b""))


def write_sources():
    base = ROOT / "sources"
    base.mkdir(parents=True, exist_ok=True)
    items = design_items()
    spec = ("Spécification", "Text", "../text/specification.txt")
    projects = {}

    def project(name, doc_type, path, extra_docs=()):
        write_project(base / f"{name}.rqtf", [spec, ("Source", doc_type, path), *extra_docs],
                      [("Spécification", "Source")])
        projects[f"sources/{name}.rqtf"] = {"Spécification": [5, 6]}

    issues = [{"key": rid, "fields": {"summary": label, "description": f"{body}\n{covers_text(cov)}",
                                      "priority": {"name": "Medium"}}}
              for rid, label, body, _, cov in items]
    (base / "jira.json").write_text(json.dumps({"startAt": 0, "total": len(issues), "issues": issues},
                                               indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    project("jira", "Jira", "jira.json")

    redmine = ['<?xml version="1.0" encoding="UTF-8"?>',
               f'<issues total_count="{len(items)}" offset="0" limit="100" type="array">']
    for n, (rid, label, body, _, cov) in enumerate(items, start=1):
        redmine.append(f"<issue><id>{n}</id><tracker id=\"1\" name=\"Conception\"/>"
                       f"<subject>{escape(rid + ' : ' + label)}</subject>"
                       f"<description>{escape(body + ' ' + covers_text(cov))}</description>"
                       '<priority id="2" name="Normal"/></issue>')
    redmine.append("</issues>")
    write_text(base / "redmine.xml", redmine)
    project("redmine", "Redmine", "redmine.xml")

    bugs = [{"id": n, "summary": f"{rid} : {label}", "comment": f"{body} {covers_text(cov)}",
             "status": "CONFIRMED", "priority": "P2"}
            for n, (rid, label, body, _, cov) in enumerate(items, start=1)]
    (base / "bugzilla.json").write_text(json.dumps({"bugs": bugs}, indent=1, ensure_ascii=False) + "\n",
                                        encoding="utf-8")
    project("bugzilla", "Bugzilla", "bugzilla.json")

    cb = [{"id": 1000 + n, "name": f"{rid} : {label}", "description": f"{body} {covers_text(cov)}",
           "status": {"id": 1, "name": "New"}}
          for n, (rid, label, body, _, cov) in enumerate(items, start=1)]
    (base / "codebeamer.json").write_text(json.dumps({"page": 1, "pageSize": 100, "total": len(cb), "items": cb},
                                                     indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    project("codebeamer", "codebeamer", "codebeamer.json")

    jama = {"data": [{"id": 10 + n, "documentKey": rid, "itemType": 33,
                      "fields": {"name": label, "description": f"<p>{escape(body)}</p><p>{covers_text(cov)}</p>",
                                 "status": "Approved"}}
                     for n, (rid, label, body, _, cov) in enumerate(items, start=1)],
            "itemTypes": [{"id": 33, "display": "Conception"}]}
    (base / "jama.json").write_text(json.dumps(jama, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    project("jama", "Jama", "jama.json")

    polarion = {"data": [{"type": "workitems", "id": f"THS/{rid}",
                          "attributes": {"id": rid, "title": label, "type": "designRequirement",
                                         "status": "draft",
                                         "description": {"type": "text/html", "value": f"<p>{escape(body)}</p>"}},
                          "relationships": {"linkedWorkItems": {"data": [
                              {"type": "linkedworkitems", "id": f"THS/{rid}/implements/THS/{t}"} for t in cov]}}}
                         for rid, label, body, _, cov in items]}
    (base / "polarion.json").write_text(json.dumps(polarion, indent=1, ensure_ascii=False) + "\n",
                                        encoding="utf-8")
    project("polarion", "Polarion", "polarion.json")

    rdf = ['<?xml version="1.0" encoding="UTF-8"?>',
           '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" '
           'xmlns:dcterms="http://purl.org/dc/terms/" xmlns:oslc_cm="http://open-services.net/ns/cm#">']
    for rid, label, body, _, cov in items:
        links = "".join(f'<oslc_cm:implementsRequirement rdf:resource="https://rm.example/req/{t}"/>' for t in cov)
        rdf.append(f'<oslc_cm:ChangeRequest rdf:about="https://ccm.example/wi/{rid}">'
                   f"<dcterms:identifier>{rid}</dcterms:identifier><dcterms:title>{escape(label)}</dcterms:title>"
                   f"<dcterms:description>{escape(body)}</dcterms:description>"
                   f"<oslc_cm:status>Open</oslc_cm:status>{links}</oslc_cm:ChangeRequest>")
    rdf.append("</rdf:RDF>")
    write_text(base / "oslc.rdf", rdf)
    project("oslc", "Change Management", "oslc.rdf")

    shapes = []
    for n, (rid, label, _, _, cov) in enumerate(items, start=1):
        prop = ""
        if cov:
            prop = ('<Section N="Property"><Row N="Prop.Trace"><Cell N="Label" V="Requirements Traceability"/>'
                    f'<Cell N="Value" V="Covers: {", ".join(cov)}"/></Row></Section>')
        shapes.append(f'<Shape ID="{n}" NameU="{rid}">{prop}<Text>{escape(label)}</Text></Shape>')
    write_zip(base / "visio.vsdx", {
        "visio/pages/pages.xml": ('<Pages xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
                                  '<Page ID="0" NameU="Architecture"><Rel r:id="rId1"/></Page></Pages>'),
        "visio/pages/_rels/pages.xml.rels": ('<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                                             '<Relationship Id="rId1" Type="http://schemas.microsoft.com/visio/2010/relationships/page" '
                                             'Target="page1.xml"/></Relationships>'),
        "visio/pages/page1.xml": f"<PageContents><Shapes>{''.join(shapes)}</Shapes></PageContents>",
    })
    project("visio", "Visio", "visio.vsdx")

    tasks = ['<?xml version="1.0" encoding="UTF-8"?>',
             '<Project xmlns="http://schemas.microsoft.com/project"><Title>Plan de conception</Title><Tasks>',
             "<Task><UID>0</UID><ID>0</ID><Name>Plan de conception</Name></Task>"]
    for n, (rid, label, body, _, cov) in enumerate(items, start=1):
        notes = body + "".join(f" Covers_{t}" for t in cov)
        tasks.append(f"<Task><UID>{n}</UID><ID>{n}</ID><Name>{rid}</Name><Notes>{escape(notes)}</Notes>"
                     "<PercentComplete>0</PercentComplete></Task>")
    tasks.append("</Tasks></Project>")
    write_text(base / "msproject.xml", tasks)
    project("msproject", "MS Project", "msproject.xml")

    xmi = ['<?xml version="1.0" encoding="UTF-8"?>',
           '<xmi:XMI xmlns:xmi="http://www.omg.org/spec/XMI/20131001" '
           'xmlns:uml="http://www.omg.org/spec/UML/20161101" '
           'xmlns:sysml="http://www.omg.org/spec/SysML/20181001/SysML">',
           '<uml:Model xmi:id="model" name="Conception du thermostat">']
    apps = []
    for rid, label, _, _, cov in items:
        xmi.append(f'<packagedElement xmi:type="uml:Class" xmi:id="{rid}" name="{rid}"/>')
        apps.append(f'<sysml:Block xmi:id="b-{rid}" base_Class="{rid}"/>')
        for t in cov:
            xmi.append(f'<packagedElement xmi:type="uml:Abstraction" xmi:id="s-{rid}-{t}" client="{rid}">'
                       f'<supplier href="specification.xmi#{t}"/></packagedElement>')
            apps.append(f'<sysml:Satisfy xmi:id="a-{rid}-{t}" base_Abstraction="s-{rid}-{t}"/>')
    xmi += ["</uml:Model>", *apps, "</xmi:XMI>"]
    write_text(base / "sysml.xmi", xmi)
    project("sysml", "SysML", "sysml.xmi")

    media = base / "media"
    media.mkdir(exist_ok=True)
    for rid, _, _, _, cov in items:
        (media / f"{rid.lower()}.png").write_bytes(png_with_comment(f"{rid} {covers_text(cov)}".strip()))
    project("media", "Media", "media")

    props = base / "proprietes"
    props.mkdir(exist_ok=True)
    for rid, label, _, _, cov in items:
        custom = ""
        if cov:
            custom = ('<?xml version="1.0" encoding="UTF-8"?><Properties xmlns="http://schemas.openxmlformats.org/'
                      'officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/'
                      'officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" '
                      f'pid="2" name="Covers"><vt:lpwstr>{", ".join(cov)}</vt:lpwstr></property></Properties>')
        parts = {"docProps/core.xml": ('<?xml version="1.0" encoding="UTF-8"?><cp:coreProperties '
                                       'xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
                                       f'xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{escape(label)}</dc:title>'
                                       "</cp:coreProperties>")}
        if custom:
            parts["docProps/custom.xml"] = custom
        write_zip(props / f"{rid.lower()}.docx", parts)
    project("proprietes", "File Properties", "proprietes")

    # Accès ZIP : la conception Word est livrée dans une archive.
    archive = base / "conception.zip"
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
        info = zipfile.ZipInfo("livraison/lisezmoi.txt", date_time=(2026, 1, 1, 0, 0, 0))
        z.writestr(info, "Archive de test ReqtiCraft.\n")
        info = zipfile.ZipInfo("livraison/design.docx", date_time=(2026, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, (ROOT / "docx" / "design.docx").read_bytes())
    project("zip", "Word", "conception.zip")
    return projects


# --- Utilitaires ------------------------------------------------------------

def write_text(path, lines):
    path.write_bytes(("\r\n".join(lines) + "\r\n").encode("utf-8"))


def write_zip(path, parts):
    # Horodatage fixe : le corpus est reproductible à l'octet près.
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, content in parts.items():
            info = zipfile.ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, content.encode("utf-8"))


def main():
    formats = {
        "text": ("Text", "txt"),
        "docx": ("Word", "docx"),
        "xlsx": ("Excel", "xlsx"),
        "pdf": ("PDF", "pdf"),
    }
    for folder, (doc_type, extension) in formats.items():
        directory = ROOT / folder
        directory.mkdir(parents=True, exist_ok=True)
        for key, (title, blocks) in DOCUMENTS.items():
            target = directory / f"{key}.{extension}"
            if folder == "text":
                write_text(target, text_lines(title, blocks))
            elif folder == "docx":
                write_docx(target, title, blocks)
            elif folder == "xlsx":
                write_xlsx(target, blocks)
            else:
                write_pdf(target, title, blocks)
        write_projects(directory, doc_type, extension)
    for folder, (doc_type, extension, writer) in FULL_FORMATS.items():
        directory = ROOT / folder
        directory.mkdir(parents=True, exist_ok=True)
        for key, (title, blocks) in DOCUMENTS.items():
            writer(directory / f"{key}.{extension}", title, blocks)
        write_project(directory / "thermostat.rqtf",
                      [(NAMES[k], doc_type, f"{k}.{extension}") for k in NAMES],
                      [(NAMES["specification"], NAMES["design"]), (NAMES["design"], NAMES["tests"])])
    write_extras()
    EXPECTED["full"] = [f"{folder}/thermostat.rqtf" for folder in FULL_FORMATS]
    EXPECTED["partial"].update(write_sources())
    (ROOT / "expected.json").write_text(
        json.dumps(EXPECTED, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
