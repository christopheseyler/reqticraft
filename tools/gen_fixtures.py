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
    write_extras()
    (ROOT / "expected.json").write_text(
        json.dumps(EXPECTED, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
