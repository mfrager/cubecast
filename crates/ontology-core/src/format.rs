//! RDF syntax registry, engine-independent.
//!
//! The enum lives in the core so import, classification, and interface layers
//! can name a format without depending on the storage adapter.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// An RDF syntax the system can parse and serialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RdfFormat {
    /// Turtle.
    Turtle,
    /// N-Triples.
    NTriples,
    /// N-Quads.
    NQuads,
    /// TriG.
    TriG,
    /// RDF/XML.
    RdfXml,
    /// JSON-LD.
    JsonLd,
}

impl RdfFormat {
    /// Every supported format.
    pub const ALL: [Self; 6] = [
        Self::Turtle,
        Self::NTriples,
        Self::NQuads,
        Self::TriG,
        Self::RdfXml,
        Self::JsonLd,
    ];

    /// Canonical short name (`turtle`, `ntriples`, …).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Turtle => "turtle",
            Self::NTriples => "ntriples",
            Self::NQuads => "nquads",
            Self::TriG => "trig",
            Self::RdfXml => "rdfxml",
            Self::JsonLd => "jsonld",
        }
    }

    /// Canonical media type.
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Turtle => "text/turtle",
            Self::NTriples => "application/n-triples",
            Self::NQuads => "application/n-quads",
            Self::TriG => "application/trig",
            Self::RdfXml => "application/rdf+xml",
            Self::JsonLd => "application/ld+json",
        }
    }

    /// Preferred file extension.
    #[must_use]
    pub const fn file_extension(self) -> &'static str {
        match self {
            Self::Turtle => "ttl",
            Self::NTriples => "nt",
            Self::NQuads => "nq",
            Self::TriG => "trig",
            Self::RdfXml => "rdf",
            Self::JsonLd => "jsonld",
        }
    }

    /// Whether the syntax can carry named graphs.
    #[must_use]
    pub const fn supports_named_graphs(self) -> bool {
        matches!(self, Self::NQuads | Self::TriG | Self::JsonLd)
    }

    /// Looks up a format from a file extension (without the dot).
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension
            .trim()
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "ttl" | "turtle" => Some(Self::Turtle),
            "nt" | "ntriples" => Some(Self::NTriples),
            "nq" | "nquads" => Some(Self::NQuads),
            "trig" => Some(Self::TriG),
            "rdf" | "xml" | "owl" | "rdfxml" => Some(Self::RdfXml),
            "jsonld" | "json-ld" | "json" => Some(Self::JsonLd),
            _ => None,
        }
    }

    /// Looks up a format from a canonical name or media type.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "turtle" | "ttl" => Some(Self::Turtle),
            "ntriples" | "n-triples" | "nt" => Some(Self::NTriples),
            "nquads" | "n-quads" | "nq" => Some(Self::NQuads),
            "trig" => Some(Self::TriG),
            "rdfxml" | "rdf/xml" | "rdf-xml" | "xml" => Some(Self::RdfXml),
            "jsonld" | "json-ld" => Some(Self::JsonLd),
            _ => Self::ALL
                .into_iter()
                .find(|format| format.media_type() == name.trim()),
        }
    }

    /// Returns the format guessed from a path's extension.
    #[must_use]
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|extension| extension.to_str())
            .and_then(Self::from_extension)
    }
}

impl fmt::Display for RdfFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RdfFormat {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::from_name(value)
            .ok_or_else(|| Error::invalid(format!("unknown RDF format {value:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats_from_extensions() {
        assert_eq!(RdfFormat::from_extension("ttl"), Some(RdfFormat::Turtle));
        assert_eq!(RdfFormat::from_extension(".NQ"), Some(RdfFormat::NQuads));
        assert_eq!(RdfFormat::from_extension("rdf"), Some(RdfFormat::RdfXml));
        assert_eq!(RdfFormat::from_extension("jsonld"), Some(RdfFormat::JsonLd));
        assert_eq!(RdfFormat::from_extension("bin"), None);
    }

    #[test]
    fn parses_names_and_media_types() {
        assert_eq!(
            "application/sparql-results+json".parse::<RdfFormat>().ok(),
            None
        );
        assert_eq!(
            "text/turtle".parse::<RdfFormat>().unwrap(),
            RdfFormat::Turtle
        );
        assert_eq!("TriG".parse::<RdfFormat>().unwrap(), RdfFormat::TriG);
    }

    #[test]
    fn named_graph_capability_is_reported() {
        assert!(RdfFormat::NQuads.supports_named_graphs());
        assert!(RdfFormat::TriG.supports_named_graphs());
        assert!(!RdfFormat::Turtle.supports_named_graphs());
        assert!(!RdfFormat::NTriples.supports_named_graphs());
    }
}
