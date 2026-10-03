//! Kleine Helfer für roxmltree: Suche nach lokalem Namen, Namespaces werden ignoriert
//! (außer bei `r:embed`/`r:id`, die über [`rel_attr`] gelesen werden).

use roxmltree::Node;

pub const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub fn children<'a, 'i>(n: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(Node::is_element)
}

pub fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    children(n).find(|c| c.tag_name().name() == name)
}

/// Folgt einem Pfad aus lokalen Namen, z. B. `["spPr", "xfrm", "off"]`.
pub fn path<'a, 'i>(n: Node<'a, 'i>, names: &[&str]) -> Option<Node<'a, 'i>> {
    names.iter().try_fold(n, |cur, name| child(cur, name))
}

pub fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute(name)
}

pub fn attr_i64(n: Node, name: &str) -> Option<i64> {
    n.attribute(name)?.parse().ok()
}

pub fn rel_attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute((R_NS, name))
}

/// OOXML-Wahrheitswerte: `1`, `true`, `on`.
pub fn attr_bool(n: Node, name: &str) -> Option<bool> {
    n.attribute(name).map(|v| matches!(v, "1" | "true" | "on"))
}
