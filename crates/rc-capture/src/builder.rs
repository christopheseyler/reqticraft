//! Construction de l'arbre des éléments, commune aux captures texte et XML.

use rc_model::{
    AttributeValue, CapturedDocument, ElemId, Element, ElementClass, ElementType, Message,
    Reference, Severity,
};

/// Champs extraits d'une correspondance.
#[derive(Debug, Default, Clone)]
pub struct Fields {
    pub ident: Option<String>,
    pub label: Option<String>,
    pub text: Option<String>,
    pub guid: Option<String>,
    pub parent: Option<String>,
    /// Niveau de section capturé (rôle « profondeur »).
    pub depth: Option<u32>,
}

/// Propriétaire d'une information rattachée (attribut, référence, texte).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Document,
    Element(ElemId),
}

pub struct Builder {
    pub doc: CapturedDocument,
}

impl Builder {
    pub fn new(document: &str, type_name: &str) -> Builder {
        Builder {
            doc: CapturedDocument {
                name: document.to_owned(),
                type_name: type_name.to_owned(),
                ..Default::default()
            },
        }
    }

    pub fn message(
        &mut self,
        rule: &str,
        severity: Severity,
        text: String,
        element: Option<ElemId>,
        line: Option<usize>,
    ) {
        let document = Some(self.doc.name.clone());
        self.doc.messages.push(Message {
            rule: rule.to_owned(),
            severity,
            text,
            document,
            element,
            line,
        });
    }

    /// Crée un élément structurel sous `parent`.
    pub fn open(
        &mut self,
        et: &ElementType,
        fields: Fields,
        parent: Option<ElemId>,
        line: usize,
    ) -> ElemId {
        let mut e = Element::new(&et.name, et.class);
        e.ident = fields.ident;
        e.label = fields.label;
        e.guid = fields.guid;
        if let Some(t) = fields.text {
            e.text = t;
        }
        e.line = line;
        e.parent = parent;
        let id = self.doc.elements.len();
        self.doc.elements.push(e);
        match parent {
            Some(p) => self.doc.elements[p].children.push(id),
            None => self.doc.roots.push(id),
        }
        id
    }

    /// Rattache une information non structurelle à son propriétaire.
    pub fn attach(
        &mut self,
        et: &ElementType,
        owner: Owner,
        fields: Fields,
        targets: Vec<String>,
        line: usize,
    ) {
        match et.class {
            ElementClass::Attribute | ElementClass::Value => {
                let value = fields.ident.or(fields.text).or(fields.label);
                let name = et.name.clone();
                if let (Some(v), false) = (&value, et.enum_values.is_empty()) {
                    if !et.enum_values.iter().any(|e| e == v) {
                        self.message(
                            "badAttributeValue",
                            Severity::Error,
                            format!("valeur « {v} » non autorisée pour l'attribut {name}"),
                            self.owner_elem(owner),
                            Some(line),
                        );
                    }
                }
                let attrs = self.attributes_mut(owner);
                if !et.flags.multiple && attrs.iter().any(|a| a.name == name) {
                    self.message(
                        "attributeMultipleTimesDefined",
                        Severity::Warning,
                        format!("attribut {name} défini plusieurs fois"),
                        self.owner_elem(owner),
                        Some(line),
                    );
                }
                self.attributes_mut(owner)
                    .push(AttributeValue { name, value });
            }
            ElementClass::Reference | ElementClass::Link => {
                for target in targets {
                    let r = Reference {
                        type_name: et.name.clone(),
                        target,
                        target_document: None,
                        inverse: et.flags.inverse,
                        attributes: Vec::new(),
                        line,
                    };
                    match (owner, et.class) {
                        (Owner::Element(e), ElementClass::Link) => {
                            self.doc.elements[e].links.push(r)
                        }
                        (Owner::Element(e), _) => self.doc.elements[e].references.push(r),
                        (Owner::Document, _) => self.doc.references.push(r),
                    }
                }
            }
            ElementClass::ReferenceAttribute => {
                let value = fields.ident.or(fields.text);
                let last = match owner {
                    Owner::Element(e) => self.doc.elements[e].references.last_mut(),
                    Owner::Document => self.doc.references.last_mut(),
                };
                match last {
                    Some(r) => r.attributes.push(AttributeValue {
                        name: et.name.clone(),
                        value,
                    }),
                    None => self.message(
                        "referenceAttributeWithoutReference",
                        Severity::Warning,
                        format!("attribut de référence {} sans référence", et.name),
                        self.owner_elem(owner),
                        Some(line),
                    ),
                }
            }
            ElementClass::Text => {
                let Some(text) = fields.text.or(fields.ident).filter(|t| !t.is_empty()) else {
                    return;
                };
                let target = match owner {
                    Owner::Element(e) => &mut self.doc.elements[e].text,
                    Owner::Document => &mut self.doc.text,
                };
                if !target.is_empty() {
                    target.push('\n');
                }
                target.push_str(&text);
            }
            // Images et tableaux : pas encore restitués.
            _ => {}
        }
    }

    fn owner_elem(&self, owner: Owner) -> Option<ElemId> {
        match owner {
            Owner::Element(e) => Some(e),
            Owner::Document => None,
        }
    }

    fn attributes_mut(&mut self, owner: Owner) -> &mut Vec<AttributeValue> {
        match owner {
            Owner::Element(e) => &mut self.doc.elements[e].attributes,
            Owner::Document => &mut self.doc.attributes,
        }
    }

    /// Retire des éléments (et leurs descendants) puis renumérote.
    pub fn remove(&mut self, removed: &[ElemId]) {
        if removed.is_empty() {
            return;
        }
        let mut gone = vec![false; self.doc.elements.len()];
        for &r in removed {
            gone[r] = true;
            for d in self.doc.descendants(r) {
                gone[d] = true;
            }
        }
        let mut map = vec![None; gone.len()];
        let mut next = 0;
        for (i, g) in gone.iter().enumerate() {
            if !g {
                map[i] = Some(next);
                next += 1;
            }
        }
        let old = std::mem::take(&mut self.doc.elements);
        for (i, mut e) in old.into_iter().enumerate() {
            if gone[i] {
                continue;
            }
            e.parent = e.parent.and_then(|p| map[p]);
            e.children = e.children.iter().filter_map(|&c| map[c]).collect();
            self.doc.elements.push(e);
        }
        self.doc.roots = self.doc.roots.iter().filter_map(|&r| map[r]).collect();
        for m in &mut self.doc.messages {
            m.element = m.element.and_then(|e| map[e]);
        }
    }
}

/// Nettoie une valeur capturée.
pub fn clean(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty()).then(|| v.to_owned())
}
