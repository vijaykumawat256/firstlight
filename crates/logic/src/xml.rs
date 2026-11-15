//! A tiny namespace-agnostic XML tree, enough for WebDAV `multistatus` responses.
//! Local names only (`d:href` and `D:href` are both `href`). Entities beyond the five
//! predefined ones and numeric references are never expanded, so entity-bomb documents are inert.

use crate::{Error, Result};
use quick_xml::XmlVersion;
use quick_xml::escape::resolve_xml_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

const MAX_DEPTH: usize = 48;

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// First descendant (depth-first) with this local name, including self.
    pub fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }
    pub fn trimmed(&self) -> &str {
        self.text.trim()
    }
}

fn bad(_: impl std::fmt::Debug) -> Error {
    Error::Provider("the server sent a response we could not read".into())
}

fn open(e: &quick_xml::events::BytesStart<'_>) -> Node {
    let attrs = e
        .attributes()
        .flatten()
        .map(|a| (a.key.local_name().as_ref().to_string(), a.value.to_string()))
        .collect();
    Node {
        name: e.local_name().as_ref().to_string(),
        attrs,
        ..Default::default()
    }
}

pub fn parse(xml: &str) -> Result<Node> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Node> = vec![Node {
        name: "#document".into(),
        ..Default::default()
    }];
    loop {
        match reader.read_event().map_err(bad)? {
            Event::Start(e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(bad("too deep"));
                }
                stack.push(open(&e));
            }
            Event::Empty(e) => stack.last_mut().expect("root").children.push(open(&e)),
            Event::End(_) => {
                let done = stack.pop().ok_or_else(|| bad("unbalanced"))?;
                stack
                    .last_mut()
                    .ok_or_else(|| bad("unbalanced"))?
                    .children
                    .push(done);
            }
            Event::Text(t) => stack
                .last_mut()
                .expect("root")
                .text
                .push_str(&t.xml_content(XmlVersion::Implicit1_0)),
            Event::CData(c) => stack
                .last_mut()
                .expect("root")
                .text
                .push_str(&c.xml_content(XmlVersion::Implicit1_0)),
            Event::GeneralRef(r) => {
                let name = r.into_inner();
                let top = stack.last_mut().expect("root");
                if let Some(num) = name.strip_prefix('#') {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16),
                        None => num.parse(),
                    };
                    if let Some(c) = code.ok().and_then(char::from_u32) {
                        top.text.push(c);
                    }
                } else if let Some(s) = resolve_xml_entity(&name) {
                    top.text.push_str(s);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if stack.len() != 1 {
        return Err(bad("unclosed element"));
    }
    let doc = stack.pop().expect("root");
    doc.children
        .into_iter()
        .next()
        .ok_or_else(|| bad("empty document"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_namespaces_attributes_text_and_entities() {
        let n = parse(
            r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/a&amp;b/</D:href>
               <C:comp xmlns:C="urn:x" name="VEVENT"/><D:note><![CDATA[<raw>]]></D:note><D:x>&#65;&#x42;</D:x></D:response></D:multistatus>"#,
        )
        .unwrap();
        assert_eq!(n.name, "multistatus");
        let r = n.child("response").unwrap();
        assert_eq!(r.child("href").unwrap().trimmed(), "/a&b/");
        assert_eq!(r.child("comp").unwrap().attr("name"), Some("VEVENT"));
        assert_eq!(r.child("note").unwrap().text, "<raw>");
        assert_eq!(r.child("x").unwrap().text, "AB");
        assert!(n.find("href").is_some());
    }

    #[test]
    fn rejects_malformed_and_too_deep_documents() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("").is_err());
        assert!(parse("not xml at all").is_err());
        let deep = format!("{}{}", "<a>".repeat(100), "</a>".repeat(100));
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn custom_entities_are_not_expanded() {
        let n = parse(r#"<!DOCTYPE x [<!ENTITY boom "AAAAAAAAAA">]><x>&boom;&boom;</x>"#).unwrap();
        assert_eq!(n.text, "", "unknown entities are dropped, never expanded");
    }
}
