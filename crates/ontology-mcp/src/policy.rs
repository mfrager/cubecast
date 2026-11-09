//! Agent permissions and audit logging.
//!
//! MCP is read-only by default. Enabling writes does not implicitly enable
//! destructive operations or external imports; those have separate switches,
//! and destructive commands also require `confirm: true` on the individual
//! request.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use chrono::Utc;
use ontology_core::command::{
    Command, JobCommand, OntologyCommand, RdfCommand, SemanticCommand, TaxonomyCommand,
};
use ontology_core::error::{Error, Result};
use ontology_core::job::JobRequest;
use ontology_core::semantic::ApplyPolicy;
use serde_json::json;

/// Whether the server may modify the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    /// Queries and read-only semantic previews only.
    ReadOnly,
    /// Writes allowed, subject to the separate destructive/external gates.
    ReadWrite,
}

impl AccessMode {
    /// Parses `ONTO_MCP_ACCESS`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] for an unknown value.
    pub fn from_env(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "" | "read-only" | "readonly" | "read_only" => Ok(Self::ReadOnly),
            "read-write" | "readwrite" | "read_write" | "write" => Ok(Self::ReadWrite),
            other => Err(Error::Config {
                detail: format!("ONTO_MCP_ACCESS must be read-only or read-write, got {other:?}"),
            }),
        }
    }
}

/// Runtime safety policy for the MCP server.
#[derive(Debug, Clone)]
pub struct Policy {
    /// Read-only or read-write.
    pub access: AccessMode,
    /// Permit deletions and SPARQL updates when writes are enabled.
    pub allow_destructive: bool,
    /// Permit external ontology imports and synchronization.
    pub allow_external: bool,
    /// Maximum rows or items returned in one result.
    pub max_results: usize,
    /// Maximum serialized RDF bytes returned inline.
    pub max_export_bytes: usize,
    /// Optional JSONL audit log.
    pub audit_log: Option<PathBuf>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            access: AccessMode::ReadOnly,
            allow_destructive: false,
            allow_external: false,
            max_results: 100,
            max_export_bytes: 2 * 1024 * 1024,
            audit_log: None,
        }
    }
}

impl Policy {
    /// Reads the `ONTO_MCP_*` environment variables.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Config`] when a value is malformed.
    pub fn from_env() -> Result<Self> {
        let mut policy = Self::default();
        if let Ok(value) = std::env::var("ONTO_MCP_ACCESS") {
            policy.access = AccessMode::from_env(&value)?;
        }
        policy.allow_destructive = env_flag("ONTO_MCP_ALLOW_DESTRUCTIVE");
        policy.allow_external = env_flag("ONTO_MCP_ALLOW_EXTERNAL");
        if let Ok(value) = std::env::var("ONTO_MCP_MAX_RESULTS") {
            policy.max_results = parse_positive(&value, "ONTO_MCP_MAX_RESULTS")?;
        }
        if let Ok(value) = std::env::var("ONTO_MCP_MAX_EXPORT_BYTES") {
            policy.max_export_bytes = parse_positive(&value, "ONTO_MCP_MAX_EXPORT_BYTES")?;
        }
        if let Ok(value) = std::env::var("ONTO_MCP_AUDIT_LOG") {
            if !value.trim().is_empty() {
                policy.audit_log = Some(PathBuf::from(value));
            }
        }
        Ok(policy)
    }

    /// Validates and authorizes a command.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] in read-only mode, and
    /// [`Error::Validation`] when a destructive, external, or approval-gated
    /// action was not explicitly enabled.
    pub fn authorize(&self, command: &Command, confirm: bool) -> Result<()> {
        if self.access == AccessMode::ReadOnly && writes(command) {
            return Err(Error::Unsupported {
                operation: "write in read-only MCP mode (set ONTO_MCP_ACCESS=read-write)"
                    .to_owned(),
            });
        }
        if destructive(command) && !self.allow_destructive {
            return Err(Error::Unsupported {
                operation: "destructive action (set ONTO_MCP_ALLOW_DESTRUCTIVE=true)".to_owned(),
            });
        }
        if needs_confirmation(command) && !confirm {
            return Err(Error::validation(
                "this action requires \"confirm\": true; use dry_run first to inspect it",
            ));
        }
        if external(command) && !self.allow_external {
            return Err(Error::Unsupported {
                operation: "external ontology access (set ONTO_MCP_ALLOW_EXTERNAL=true)".to_owned(),
            });
        }
        Ok(())
    }

    /// Appends one JSON object to the configured audit log.
    ///
    /// Audit failures are logged but never fail the request.
    pub fn audit(
        &self,
        tool: &str,
        action: &str,
        target: Option<&str>,
        command: Option<&Command>,
        outcome: &str,
    ) {
        let Some(path) = &self.audit_log else {
            return;
        };
        let record = json!({
            "timestamp": Utc::now(),
            "tool": tool,
            "action": action,
            "target": target,
            "command": command.map(|command| format!("{command:?}")),
            "outcome": outcome,
        });
        let line = match serde_json::to_string(&record) {
            Ok(line) => line,
            Err(error) => {
                tracing::warn!(%error, "could not encode MCP audit record");
                return;
            }
        };
        let result = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if let Err(error) = result {
            tracing::warn!(%error, path = %path.display(), "could not write MCP audit log");
        }
    }
}

fn env_flag(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn parse_positive(value: &str, name: &str) -> Result<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::Config {
            detail: format!("{name} must be a positive integer, got {value:?}"),
        })
}

/// Whether the command changes the graph.
#[must_use]
pub fn writes(command: &Command) -> bool {
    match command {
        Command::Rdf(command) => match command {
            RdfCommand::GetInstance { .. }
            | RdfCommand::ListInstances(_)
            | RdfCommand::ListStatements { .. }
            | RdfCommand::GetRelationships { .. }
            | RdfCommand::Query { .. }
            | RdfCommand::ListGraphs
            | RdfCommand::Export { .. } => false,
            RdfCommand::CreateInstance(_)
            | RdfCommand::UpdateInstance { .. }
            | RdfCommand::DeleteInstance { .. }
            | RdfCommand::AddStatements { .. }
            | RdfCommand::DeleteStatements { .. }
            | RdfCommand::Update { .. }
            | RdfCommand::Import { .. }
            | RdfCommand::ClearGraph { .. } => true,
        },
        Command::Ontology(command) => !matches!(
            command,
            OntologyCommand::GetClass { .. }
                | OntologyCommand::ListClasses(_)
                | OntologyCommand::GetProperty { .. }
                | OntologyCommand::ListProperties(_)
                | OntologyCommand::ClassHierarchy { .. }
                | OntologyCommand::Validate { .. }
                | OntologyCommand::GetOntology { .. }
                | OntologyCommand::ListOntologies
                | OntologyCommand::OntologyVersions { .. }
                | OntologyCommand::ApplicableProperties { .. }
                | OntologyCommand::ApplicableClasses { .. }
        ),
        Command::Taxonomy(command) => !matches!(
            command,
            TaxonomyCommand::GetScheme { .. }
                | TaxonomyCommand::ListSchemes
                | TaxonomyCommand::SchemeVersions { .. }
                | TaxonomyCommand::GetConcept { .. }
                | TaxonomyCommand::ListConcepts(_)
                | TaxonomyCommand::SearchConcepts { .. }
                | TaxonomyCommand::ListRelations { .. }
                | TaxonomyCommand::Descendants { .. }
                | TaxonomyCommand::Ancestors { .. }
                | TaxonomyCommand::ConceptTree { .. }
                | TaxonomyCommand::ValidateTaxonomy { .. }
                | TaxonomyCommand::GetCollection { .. }
                | TaxonomyCommand::ListCollections { .. }
        ),
        Command::External(command) => matches!(
            command,
            ontology_core::command::ExternalCommand::Import(_)
                | ontology_core::command::ExternalCommand::Sync { .. }
                | ontology_core::command::ExternalCommand::Delete { .. }
                | ontology_core::command::ExternalCommand::MapClass(_)
                | ontology_core::command::ExternalCommand::MapProperty(_)
        ),
        Command::Semantic(command) => match command {
            SemanticCommand::Classify { options, .. }
            | SemanticCommand::ClassifyBatch { options, .. } => options.persist,
            SemanticCommand::ListClassifications { .. }
            | SemanticCommand::SearchSimilar { .. }
            | SemanticCommand::ListEmbeddings { .. }
            | SemanticCommand::PreviewInference { .. }
            | SemanticCommand::ExplainInference { .. } => false,
            SemanticCommand::OverrideClassification { .. }
            | SemanticCommand::ReviewClassification { .. }
            | SemanticCommand::Reclassify { .. }
            | SemanticCommand::GenerateEmbeddings { .. }
            | SemanticCommand::DeleteEmbeddings { .. } => true,
            SemanticCommand::ApplyInference { policy, .. } => {
                !matches!(policy, ApplyPolicy::Suggest)
            }
        },
        Command::Job(JobCommand::Submit(JobRequest::RdfExport { .. })) => false,
        Command::Job(JobCommand::Submit(_)) => true,
        Command::Job(_) => false,
        // Extraction may write the RDF mapping; loading a stored run does not.
        Command::Document(ontology_core::command::DocumentCommand::Extract(_)) => true,
        Command::Document(ontology_core::command::DocumentCommand::Get { .. }) => false,
    }
}

/// Whether the command removes data or runs arbitrary SPARQL Update.
#[must_use]
pub fn destructive(command: &Command) -> bool {
    match command {
        Command::Rdf(
            RdfCommand::DeleteInstance { .. }
            | RdfCommand::DeleteStatements { .. }
            | RdfCommand::Update { .. }
            | RdfCommand::ClearGraph { .. },
        ) => true,
        Command::Ontology(
            OntologyCommand::DeleteClass { .. }
            | OntologyCommand::DeleteProperty { .. }
            | OntologyCommand::DeleteOntology { .. },
        ) => true,
        Command::Taxonomy(
            TaxonomyCommand::DeleteScheme { .. }
            | TaxonomyCommand::DeleteConcept { .. }
            | TaxonomyCommand::DeleteCollection { .. }
            | TaxonomyCommand::MergeSchemes { .. },
        ) => true,
        Command::External(
            ontology_core::command::ExternalCommand::Delete { .. }
            | ontology_core::command::ExternalCommand::Import(_)
            | ontology_core::command::ExternalCommand::Sync { .. },
        ) => true,
        Command::Semantic(SemanticCommand::DeleteEmbeddings { .. }) => true,
        _ => false,
    }
}

/// Whether the command needs an explicit inline confirmation.
#[must_use]
pub fn needs_confirmation(command: &Command) -> bool {
    if destructive(command) {
        return true;
    }
    if destructive(command) {
        return true;
    }
    matches!(
        command,
        Command::Semantic(SemanticCommand::ApplyInference { policy, .. })
            if !matches!(policy, ApplyPolicy::Suggest)
    ) || matches!(
        command,
        Command::Job(JobCommand::Submit(
            JobRequest::ExternalImport(_) | JobRequest::ExternalSync { .. }
        ))
    )
}

/// Whether the command reaches outside the local graph.
#[must_use]
pub fn external(command: &Command) -> bool {
    matches!(
        command,
        Command::External(ontology_core::command::ExternalCommand::Import(_))
            | Command::External(ontology_core::command::ExternalCommand::Sync { .. })
            | Command::Job(JobCommand::Submit(
                JobRequest::ExternalImport(_) | JobRequest::ExternalSync { .. }
            ))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ontology_core::command::{JobCommand, RdfCommand};
    use ontology_core::job::JobRequest;

    #[test]
    fn read_only_blocks_writes_but_allows_queries() {
        let policy = Policy::default();
        assert!(policy
            .authorize(&Command::Rdf(RdfCommand::ListGraphs), false)
            .is_ok());
        assert!(policy
            .authorize(
                &Command::Rdf(RdfCommand::Query {
                    sparql: "ASK {}".to_owned()
                }),
                false
            )
            .is_ok());
        assert!(policy
            .authorize(
                &Command::Rdf(RdfCommand::ClearGraph {
                    graph: ontology_core::model::GraphName::Default
                }),
                true
            )
            .is_err());
    }

    #[test]
    fn destructive_actions_need_both_switches_and_confirmation() {
        let command = Command::Rdf(RdfCommand::DeleteInstance {
            id: ontology_core::id::Id::new("urn:example:x").unwrap(),
            graph: None,
        });
        let read_write = Policy {
            access: AccessMode::ReadWrite,
            ..Policy::default()
        };
        assert!(read_write.authorize(&command, true).is_err());

        let destructive = Policy {
            allow_destructive: true,
            ..read_write
        };
        assert!(destructive.authorize(&command, false).is_err());
        assert!(destructive.authorize(&command, true).is_ok());
    }

    #[test]
    fn external_imports_are_separately_gated() {
        let command = Command::External(ontology_core::command::ExternalCommand::Import(
            ontology_core::external::ImportRequest::new("https://example.org/schema"),
        ));
        let policy = Policy {
            access: AccessMode::ReadWrite,
            allow_destructive: true,
            ..Policy::default()
        };
        assert!(policy.authorize(&command, true).is_err());
        let allowed = Policy {
            allow_external: true,
            ..policy
        };
        assert!(allowed.authorize(&command, true).is_ok());
    }

    #[test]
    fn read_only_allows_export_jobs_but_blocks_writes() {
        let export = Command::Job(JobCommand::Submit(JobRequest::RdfExport {
            format: "turtle".to_owned(),
            graph: None,
        }));
        assert!(Policy::default().authorize(&export, false).is_ok());

        let import = Command::Job(JobCommand::Submit(JobRequest::RdfImport {
            format: "turtle".to_owned(),
            data: String::new(),
            graph: None,
        }));
        assert!(Policy::default().authorize(&import, false).is_err());
    }

    #[test]
    fn suggest_inference_is_read_only() {
        let command = Command::Semantic(SemanticCommand::ApplyInference {
            instance: ontology_core::id::Id::new("urn:example:x").unwrap(),
            options: ontology_core::semantic::InferenceOptions::default(),
            policy: ApplyPolicy::Suggest,
        });
        assert!(Policy::default().authorize(&command, false).is_ok());
    }
}
