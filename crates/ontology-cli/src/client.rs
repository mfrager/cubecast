//! HTTP transport and output rendering for the CLI.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use clap::ValueEnum;
use ontology_core::command::Command;
use ontology_core::id::Id;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cli::Connection;
use crate::CliResult;

/// Result rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    /// Pretty-printed JSON.
    Json,
    /// YAML.
    Yaml,
    /// Compact text table.
    Table,
}

/// One command plus an optional file destination for exported RDF.
#[derive(Debug)]
pub struct Invocation {
    /// Command to execute.
    pub command: Command,
    /// Where to write an exported document.
    pub output: Option<PathBuf>,
}

impl Invocation {
    /// Wraps a command.
    #[must_use]
    pub fn new(command: Command) -> Self {
        Self {
            command,
            output: None,
        }
    }

    /// Wraps a command and an export destination.
    #[must_use]
    pub fn with_output(command: Command, output: Option<PathBuf>) -> Self {
        Self { command, output }
    }
}

impl Connection {
    /// Executes one command and renders its result.
    ///
    /// # Errors
    ///
    /// Propagates HTTP, serialization, and I/O failures.
    pub async fn execute(&self, invocation: Invocation) -> CliResult<()> {
        if self.dry_run {
            let json = serde_json::to_value(&invocation.command)?;
            println!("{}", serde_json::to_string_pretty(&json)?);
            return Ok(());
        }
        if let Some(path) = invocation.output {
            let value = self.send(&invocation.command).await?;
            if let Some(data) = value.get("data").and_then(Value::as_str) {
                std::fs::write(&path, data)?;
                println!("wrote {}", path.display());
                return Ok(());
            }
            return Err("the server did not return serialized RDF".into());
        }
        let value = self.send(&invocation.command).await?;
        self.print(&value)
    }

    /// Downloads one distribution artifact from the registry.
    ///
    /// # Errors
    ///
    /// Returns an error for transport failures, non-success responses, or I/O.
    pub async fn download(
        &self,
        key: &str,
        format: &str,
        output: &std::path::Path,
    ) -> CliResult<()> {
        if self.dry_run {
            println!(
                "GET {}/api/v1/registry/download/{key}/{format}",
                self.server.trim_end_matches('/')
            );
            return Ok(());
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let url = format!(
            "{}/api/v1/registry/download/{key}/{format}",
            self.server.trim_end_matches('/')
        );
        let mut request = client.get(url);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("API returned {status}: {}", response.text().await?).into());
        }
        let bytes = response.bytes().await?;
        std::fs::write(output, &bytes)?;
        println!("wrote {}", output.display());
        Ok(())
    }

    /// Posts a command and returns the decoded result.
    ///
    /// # Errors
    ///
    /// Returns an error for transport failures and non-success responses.
    pub async fn send(&self, command: &Command) -> CliResult<Value> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(300))
            .build()?;
        let url = format!("{}/api/v1/commands", self.server.trim_end_matches('/'));
        let mut request = client.post(url).json(command);
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(format!("API returned {status}: {text}").into());
        }
        Ok(serde_json::from_str(&text)?)
    }

    /// Prints a value in the configured format.
    ///
    /// # Errors
    ///
    /// Propagates serialization failures.
    pub fn print(&self, value: &Value) -> CliResult<()> {
        match self.output {
            OutputFormat::Json => println!("{}", serde_json::to_string_pretty(value)?),
            OutputFormat::Yaml => print!("{}", serde_yaml::to_string(value)?),
            OutputFormat::Table => println!("{}", render_table(value)),
        }
        Ok(())
    }

    /// Asks for confirmation unless `--yes` was passed or stdin is not a
    /// terminal.
    ///
    /// # Errors
    ///
    /// Returns an error when the user declines or reading stdin fails.
    pub fn confirm(&self, prompt: &str) -> CliResult<()> {
        if self.assume_yes || !std::io::stdin().is_terminal() {
            return Ok(());
        }
        eprint!("{prompt} [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_to_string(&mut answer)?;
        if matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes") {
            Ok(())
        } else {
            Err("aborted".into())
        }
    }
}

fn render_table(value: &Value) -> String {
    if let Some(Value::Array(items)) = value.get("items") {
        return render_rows(items, 3);
    }
    if let Some(Value::Array(items)) = value
        .get("resource")
        .and_then(|resource| resource.get("items"))
    {
        return render_rows(items, 3);
    }
    let rows = match value {
        Value::Array(items) => items.iter().collect::<Vec<_>>(),
        other => vec![other],
    };
    if rows.len() > 1 {
        let owned: Vec<Value> = rows.into_iter().cloned().collect();
        return render_rows(&owned, 2);
    }
    match rows.first() {
        Some(Value::Object(map)) => map
            .iter()
            .map(|(key, value)| format!("{key:<24} {}", compact(value)))
            .collect::<Vec<_>>()
            .join("\n"),
        Some(value) => compact(value),
        None => String::new(),
    }
}

fn render_rows(items: &[Value], _indent: usize) -> String {
    let mut columns: Vec<String> = Vec::new();
    for item in items {
        if let Value::Object(map) = item {
            for key in map.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
    }
    if columns.is_empty() {
        return items.iter().map(compact).collect::<Vec<_>>().join("\n");
    }
    let mut output = String::new();
    output.push_str(&columns.join("\t"));
    output.push('\n');
    for item in items {
        let cells = columns
            .iter()
            .map(|column| {
                item.get(column)
                    .map(compact)
                    .unwrap_or_default()
                    .replace(['\n', '\r', '\t'], " ")
            })
            .collect::<Vec<_>>();
        output.push_str(&cells.join("\t"));
        output.push('\n');
    }
    output
}

fn compact(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Resolves `--server`, `--api-key`, environment, and profiles.
///
/// # Errors
///
/// Returns an error when an explicitly selected profile is unknown or the
/// profile file is malformed.
pub fn resolve(cli: &crate::cli::Cli) -> CliResult<Connection> {
    let profile_name = cli.profile.clone().unwrap_or_else(|| "default".to_owned());
    let profile = read_profile(&profile_name)?;
    if cli.profile.is_some() && profile.is_none() {
        return Err(format!("unknown profile `{profile_name}`").into());
    }
    let profile = profile.unwrap_or_default();
    Ok(Connection {
        server: cli
            .server
            .clone()
            .or(profile.server)
            .unwrap_or_else(|| "http://127.0.0.1:8080".to_owned()),
        api_key: cli.api_key.clone().or(profile.api_key),
        output: cli.output.or(profile.output).unwrap_or(OutputFormat::Json),
        dry_run: cli.dry_run,
        assume_yes: cli.yes,
    })
}

#[derive(Debug, Default, Deserialize)]
struct ProfileFile {
    #[serde(default)]
    profiles: std::collections::BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Profile {
    server: Option<String>,
    api_key: Option<String>,
    output: Option<OutputFormat>,
}

fn read_profile(name: &str) -> CliResult<Option<Profile>> {
    let path = match std::env::var_os("ONTO_CLI_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(".config/onto/cli.toml"),
            None => return Ok(None),
        },
    };
    if !path.exists() {
        return Ok(None);
    }
    let document = std::fs::read_to_string(&path)?;
    let file: ProfileFile = toml::from_str(&document)?;
    Ok(file.profiles.get(name).cloned())
}

/// Returns the default job identifier type used by `watch`.
pub type WatchId = Id;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn renders_collection_tables() {
        let table = render_table(&json!({
            "items": [
                { "id": "urn:example:a", "name": "A" },
                { "id": "urn:example:b", "name": "B" }
            ]
        }));
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines[0], "id\tname");
        assert!(lines[1].contains("urn:example:a\tA"));
        assert!(lines[2].contains("urn:example:b\tB"));
    }

    #[test]
    fn renders_nested_resource_items() {
        let table = render_table(&json!({
            "resource": { "items": [{ "id": "urn:example:a" }] }
        }));
        assert!(table.contains("id"));
        assert!(table.contains("urn:example:a"));
    }

    #[test]
    fn renders_single_objects_as_key_value_lines() {
        let table = render_table(&json!({ "id": "urn:example:a", "count": 2 }));
        assert!(table.contains("id"));
        assert!(table.contains("urn:example:a"));
        assert!(table.contains("count"));
        assert!(table.contains('2'));
    }

    #[test]
    fn renders_scalars_without_headers() {
        assert_eq!(render_table(&json!("plain")), "plain");
        assert_eq!(render_table(&json!(null)), "");
    }
}
