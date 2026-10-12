//! Shared XML → tree parse for plugin pages and components.

use std::path::{Path, PathBuf};

use quick_xml::{Reader, events::Event};
use serde_json::{Map, Value};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XmlError {
    #[error("{path}: invalid XML: {message}")]
    Xml { path: PathBuf, message: String },
}

/// One element in a page or component XML file.
#[derive(Debug)]
pub struct Node {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: Option<String>,
}

impl Node {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn collect_models(&self, models: &mut std::collections::BTreeSet<String>) {
        if let Some(model) = self.attribute("model").filter(|model| !model.is_empty()) {
            models.insert(model.to_string());
        }
        for child in &self.children {
            child.collect_models(models);
        }
    }

    /// `{ "type": <tag>, <attributes…>, "children": […], "text": … }`.
    ///
    /// The renderer reads the element name from `type`, so an XML `type`
    /// attribute (as on `<view type="list">`) becomes `viewType`. The literals
    /// `true` and `false` become JSON booleans.
    pub fn into_json(self) -> Value {
        let mut object = Map::new();
        for (key, value) in self.attributes {
            let key = if key == "type" {
                "viewType".to_string()
            } else {
                key
            };
            let value = match value.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => Value::String(value),
            };
            object.insert(key, value);
        }
        object.insert("type".into(), Value::String(self.tag));
        if !self.children.is_empty() {
            object.insert(
                "children".into(),
                Value::Array(self.children.into_iter().map(Node::into_json).collect()),
            );
        }
        if let Some(text) = self.text {
            object.insert("text".into(), Value::String(text));
        }
        Value::Object(object)
    }
}

/// Parse an XML document into a single root [`Node`].
pub fn compile_xml(xml: &str, source: &Path) -> Result<Node, XmlError> {
    let xml_error = |message: String| XmlError::Xml {
        path: source.to_path_buf(),
        message,
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<Node> = Vec::new();
    let mut root: Option<Node> = None;

    let read_attributes = |element: &quick_xml::events::BytesStart<'_>,
                           decoder: quick_xml::encoding::Decoder|
     -> Result<Vec<(String, String)>, XmlError> {
        let mut attributes = Vec::new();
        for attribute in element.attributes() {
            let attribute = attribute.map_err(|error| xml_error(error.to_string()))?;
            let key = String::from_utf8_lossy(attribute.key.as_ref()).into_owned();
            let value = attribute
                .decode_and_unescape_value(decoder)
                .map_err(|error| xml_error(error.to_string()))?
                .into_owned();
            attributes.push((key, value));
        }
        Ok(attributes)
    };
    let append = |stack: &mut Vec<Node>, root: &mut Option<Node>, node: Node| {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(node);
            Ok(())
        } else if root.replace(node).is_some() {
            Err(xml_error("the file must have exactly one root element".into()))
        } else {
            Ok(())
        }
    };

    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                let attributes = read_attributes(&element, reader.decoder())?;
                stack.push(Node {
                    tag: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
                    attributes,
                    children: Vec::new(),
                    text: None,
                });
            }
            Ok(Event::Empty(element)) => {
                let attributes = read_attributes(&element, reader.decoder())?;
                let node = Node {
                    tag: String::from_utf8_lossy(element.name().as_ref()).into_owned(),
                    attributes,
                    children: Vec::new(),
                    text: None,
                };
                append(&mut stack, &mut root, node)?;
            }
            Ok(Event::End(_)) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| xml_error("unexpected closing element".into()))?;
                append(&mut stack, &mut root, node)?;
            }
            Ok(Event::Text(text)) => {
                let text = text.decode().map_err(|error| xml_error(error.to_string()))?;
                if let Some(node) = stack.last_mut()
                    && !text.trim().is_empty()
                {
                    node.text
                        .get_or_insert_with(String::new)
                        .push_str(text.as_ref());
                }
            }
            Ok(Event::CData(text)) => {
                let text = text.decode().map_err(|error| xml_error(error.to_string()))?;
                if let Some(node) = stack.last_mut() {
                    node.text
                        .get_or_insert_with(String::new)
                        .push_str(text.as_ref());
                }
            }
            Ok(Event::GeneralRef(reference)) => {
                let resolved = resolve_general_ref(&reference).map_err(xml_error)?;
                if let Some(node) = stack.last_mut() {
                    node.text.get_or_insert_with(String::new).push(resolved);
                }
            }
            Ok(Event::Eof) => break,
            Ok(Event::Decl(_) | Event::Comment(_) | Event::DocType(_) | Event::PI(_)) => {}
            Err(error) => return Err(xml_error(error.to_string())),
        }
    }

    if !stack.is_empty() {
        return Err(xml_error("unclosed elements remain".into()));
    }
    root.ok_or_else(|| xml_error("the file has no root element".into()))
}

/// Resolve a character reference (`&#38;`) or one of the five predefined XML
/// entities.
fn resolve_general_ref(reference: &quick_xml::events::BytesRef<'_>) -> Result<char, String> {
    if let Some(character) = reference
        .resolve_char_ref()
        .map_err(|error| error.to_string())?
    {
        return Ok(character);
    }
    let name = reference.decode().map_err(|error| error.to_string())?;
    match name.as_ref() {
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "amp" => Ok('&'),
        "apos" => Ok('\''),
        "quot" => Ok('"'),
        other => Err(format!("unknown XML entity `&{other};`")),
    }
}
