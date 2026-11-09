//! Central prefix registry.
//!
//! Every interface accepts prefixed names (`ex:Product`, `data:asserted`) and
//! resolves them through this registry, so the same string means the same IRI
//! in HTTP, CLI, MCP, and tests.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::id::Id;

/// RDF namespace.
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
/// `rdf:type`.
pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
/// `rdf:langString`.
pub const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
/// RDF Schema namespace.
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
/// OWL namespace.
pub const OWL: &str = "http://www.w3.org/2002/07/owl#";
/// XML Schema datatype namespace.
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
/// `xsd:string`.
pub const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
/// `xsd:integer`.
pub const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
/// `xsd:boolean`.
pub const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
/// `xsd:dateTime`.
pub const XSD_DATETIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
/// SKOS namespace.
pub const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
/// Dublin Core Terms namespace.
pub const DCTERMS: &str = "http://purl.org/dc/terms/";
/// W3C PROV namespace.
pub const PROV: &str = "http://www.w3.org/ns/prov#";
/// schema.org namespace.
pub const SCHEMA: &str = "https://schema.org/";
/// FOAF namespace.
pub const FOAF: &str = "http://xmlns.com/foaf/0.1/";
/// SHACL namespace.
pub const SH: &str = "http://www.w3.org/ns/shacl#";
/// DCAT 3 namespace.
pub const DCAT: &str = "http://www.w3.org/ns/dcat#";
/// ADMS namespace.
pub const ADMS: &str = "http://www.w3.org/ns/adms#";
/// PROF namespace.
pub const PROF: &str = "http://www.w3.org/ns/prof#";
/// VoID namespace.
pub const VOID: &str = "http://rdfs.org/ns/void#";
/// Registry extension namespace.
pub const OREG: &str = "urn:onto:registry:";
/// Project extension namespace.
pub const PM: &str = "urn:onto:project:";
/// Namespace of the system's internal OWL vocabulary.
pub const ONTO: &str = "urn:onto:schema:";
/// Namespace of reserved system graphs.
pub const SYSTEM: &str = "urn:onto:system:";
/// Namespace of data graphs.
pub const DATA: &str = "urn:onto:data:";
/// Default example namespace used by documentation and tests.
pub const EXAMPLE: &str = "https://example.org/";

/// Mutable prefix registry with the standard prefixes preloaded.
///
/// The registry is ordered so `prefixes()` is deterministic, which matters for
/// generated documentation and reproducible exports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceRegistry {
    prefixes: BTreeMap<String, String>,
}

impl Default for NamespaceRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

impl NamespaceRegistry {
    /// Creates a registry containing the standard prefixes.
    #[must_use]
    pub fn with_builtins() -> Self {
        let mut prefixes = BTreeMap::new();
        for (prefix, iri) in [
            ("rdf", RDF),
            ("rdfs", RDFS),
            ("owl", OWL),
            ("xsd", XSD),
            ("skos", SKOS),
            ("dcterms", DCTERMS),
            ("schema", SCHEMA),
            ("foaf", FOAF),
            ("prov", PROV),
            ("sh", SH),
            ("dcat", DCAT),
            ("adms", ADMS),
            ("prof", PROF),
            ("void", VOID),
            ("oreg", OREG),
            ("pm", PM),
            ("onto", ONTO),
            ("system", SYSTEM),
            ("data", DATA),
            ("ex", EXAMPLE),
        ] {
            prefixes.insert(prefix.to_owned(), iri.to_owned());
        }
        Self { prefixes }
    }

    /// Creates an empty registry.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            prefixes: BTreeMap::new(),
        }
    }

    /// Registers or replaces a prefix.
    ///
    /// # Errors
    ///
    /// Returns an error when the prefix is empty or the namespace is not an IRI.
    pub fn insert(
        &mut self,
        prefix: impl Into<String>,
        namespace: impl Into<String>,
    ) -> Result<()> {
        let prefix = prefix.into();
        if prefix.is_empty()
            || !prefix
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(Error::validation(format!("invalid prefix {prefix:?}")));
        }
        let namespace = namespace.into();
        Id::new(namespace.as_str())?;
        self.prefixes.insert(prefix, namespace);
        Ok(())
    }

    /// Returns the namespace bound to `prefix`.
    #[must_use]
    pub fn get(&self, prefix: &str) -> Option<&str> {
        self.prefixes.get(prefix).map(String::as_str)
    }

    /// Iterates over `(prefix, namespace)` pairs in prefix order.
    pub fn prefixes(&self) -> impl Iterator<Item = (&str, &str)> {
        self.prefixes
            .iter()
            .map(|(prefix, namespace)| (prefix.as_str(), namespace.as_str()))
    }

    /// Resolves a prefixed name or absolute IRI into an [`Id`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnknownPrefix`] when the name has a prefix that is not
    /// registered, and [`Error::InvalidIri`] when the result is not an IRI.
    pub fn resolve(&self, name: &str) -> Result<Id> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::invalid("identifier must not be empty"));
        }
        if let Some(stripped) = name.strip_prefix('<').and_then(|n| n.strip_suffix('>')) {
            return Id::new(stripped);
        }
        if name.starts_with("urn:") || name.contains("://") {
            return Id::new(name);
        }
        let Some((prefix, local)) = name.split_once(':') else {
            return Err(Error::invalid(format!(
                "{name:?} is neither an absolute IRI nor a prefixed name"
            )));
        };
        if local.is_empty() {
            return Err(Error::invalid(format!("{name:?} has an empty local name")));
        }
        let namespace = self
            .prefixes
            .get(prefix)
            .ok_or_else(|| Error::UnknownPrefix {
                prefix: prefix.to_owned(),
            })?;
        Id::new(format!("{namespace}{local}"))
    }

    /// Renders an [`Id`] as a prefixed name when a prefix matches, otherwise as
    /// the bare IRI.
    #[must_use]
    pub fn shorten(&self, id: &Id) -> String {
        let value = id.as_str();
        let mut best: Option<(&str, &str)> = None;
        for (prefix, namespace) in &self.prefixes {
            if let Some(local) = value.strip_prefix(namespace.as_str()) {
                if local.is_empty() {
                    continue;
                }
                let better = best.is_none_or(|(_, current)| namespace.len() > current.len());
                if better {
                    best = Some((prefix, namespace));
                }
            }
        }
        match best {
            Some((prefix, namespace)) => format!("{prefix}:{}", &value[namespace.len()..]),
            None => value.to_owned(),
        }
    }

    /// Returns the [`Id`] for a local name in the internal schema namespace.
    ///
    /// # Panics
    ///
    /// Panics only if the internal namespace constant is not a valid IRI, which
    /// is covered by a unit test.
    #[must_use]
    pub fn onto(local: &str) -> Id {
        Id::new(format!("{ONTO}{local}")).expect("the internal schema namespace is a valid IRI")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_builtin_prefixes() {
        let registry = NamespaceRegistry::with_builtins();
        assert_eq!(
            registry.resolve("ex:Product").unwrap().as_str(),
            "https://example.org/Product"
        );
        assert_eq!(
            registry.resolve("data:asserted").unwrap().as_str(),
            "urn:onto:data:asserted"
        );
        assert_eq!(
            registry.resolve("schema:name").unwrap().as_str(),
            "https://schema.org/name"
        );
    }

    #[test]
    fn passes_absolute_iris_through() {
        let registry = NamespaceRegistry::with_builtins();
        assert_eq!(
            registry
                .resolve("urn:example:product:123")
                .unwrap()
                .as_str(),
            "urn:example:product:123"
        );
        assert_eq!(
            registry
                .resolve("<https://example.org/x>")
                .unwrap()
                .as_str(),
            "https://example.org/x"
        );
    }

    #[test]
    fn rejects_unknown_prefixes() {
        let registry = NamespaceRegistry::with_builtins();
        assert!(matches!(
            registry.resolve("nope:Thing"),
            Err(Error::UnknownPrefix { .. })
        ));
    }

    #[test]
    fn shortens_with_longest_match() {
        let mut registry = NamespaceRegistry::with_builtins();
        registry.insert("onto", "urn:onto:schema:").unwrap();
        let id = Id::new("urn:onto:schema:Instance").unwrap();
        assert_eq!(registry.shorten(&id), "onto:Instance");

        let id = Id::new("https://example.org/Product").unwrap();
        assert_eq!(registry.shorten(&id), "ex:Product");
    }
}
