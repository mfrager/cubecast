//! Validated IRIs and generated operation identifiers.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A validated absolute IRI.
///
/// Every identifier that crosses a boundary (HTTP, CLI, MCP, storage) is an
/// `Id`, so malformed IRIs are rejected at the edge instead of inside a SPARQL
/// string or a database key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(String);

impl Id {
    /// Parses and validates an absolute IRI.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidIri`] when the value is empty or not an IRI.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::InvalidIri {
                value,
                detail: "identifier must not be empty".to_owned(),
            });
        }
        oxiri::Iri::parse(value.as_str()).map_err(|error| Error::InvalidIri {
            value: value.clone(),
            detail: error.to_string(),
        })?;
        Ok(Self(value))
    }

    /// Returns the IRI text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Consumes the identifier and returns the IRI text.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }

    /// Returns the final local-name segment (after the last `/`, `#`, or `:`).
    #[must_use]
    pub fn local_name(&self) -> &str {
        self.0
            .rfind(['/', '#', ':'])
            .map_or(self.0.as_str(), |index| &self.0[index + 1..])
    }

    /// Returns everything before the local name.
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.0
            .rfind(['/', '#', ':'])
            .map_or("", |index| &self.0[..=index])
    }
}

impl fmt::Display for Id {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Id {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<&str> for Id {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

impl TryFrom<String> for Id {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

/// Identifier for an application operation (used for provenance records).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// Generates a new ULID-based operation identifier.
    #[must_use]
    pub fn generate() -> Self {
        Self(ulid::Ulid::new().to_string())
    }

    /// Wraps an existing operation identifier.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Validation`] when the value is empty.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(Error::validation("operation identifier must not be empty"));
        }
        Ok(Self(value))
    }

    /// Returns the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_absolute_iris() {
        for value in [
            "https://example.org/Product",
            "urn:onto:data:asserted",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
        ] {
            assert!(Id::new(value).is_ok(), "{value} should be a valid IRI");
        }
    }

    #[test]
    fn rejects_invalid_iris() {
        assert!(Id::new("").is_err());
        assert!(Id::new("not an iri").is_err());
        assert!(Id::new("://broken").is_err());
        assert!(Id::new("https://exa mple.org/").is_err());
    }

    #[test]
    fn accepts_scheme_like_prefixed_names() {
        // `ex:Product` is a syntactically valid IRI (scheme `ex`); the namespace
        // registry is what decides that `ex:` means a registered prefix.
        assert!(Id::new("ex:Product").is_ok());
    }

    #[test]
    fn splits_local_names() {
        let id = Id::new("urn:onto:schema:Instance").unwrap();
        assert_eq!(id.local_name(), "Instance");
        assert_eq!(id.namespace(), "urn:onto:schema:");

        let id = Id::new("https://example.org/Product").unwrap();
        assert_eq!(id.local_name(), "Product");
        assert_eq!(id.namespace(), "https://example.org/");
    }

    #[test]
    fn generates_distinct_operation_ids() {
        assert_ne!(OperationId::generate(), OperationId::generate());
    }
}
