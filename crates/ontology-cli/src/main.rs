//! `onto` command-line entry point.

use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use ontology_api::ServeOptions;
use ontology_cli::cli::{self, Action, Cli, Connection};
use ontology_cli::CliResult;
use ontology_core::command::{
    Command, ExternalCommand, JobCommand, OntologyCommand, RdfCommand, SemanticCommand,
    TaxonomyCommand,
};
use ontology_core::config::Config;
use ontology_core::id::Id;
use ontology_core::tracing::init_tracing;
use serde_json::json;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("onto: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> CliResult<()> {
    let connection = Connection::resolve(&cli)?;
    let action = cli::action(cli.command)?;
    if matches!(action, Action::Shell) {
        return shell(connection).await;
    }
    perform(connection, action).await
}

async fn perform(connection: Connection, action: Action) -> CliResult<()> {
    match action {
        Action::Server(args) => {
            let path = args
                .config
                .or_else(|| std::env::var("ONTO_CONFIG").ok().map(PathBuf::from));
            let config = Config::load(path.as_deref())?;
            init_tracing(&config.logging);
            let mut options = ServeOptions::from_env(config);
            if let Some(bind) = args.bind {
                options.bind = bind;
            }
            if let Some(api_key) = connection.api_key {
                options.api_key = Some(api_key);
            }
            ontology_api::serve(options).await?;
            Ok(())
        }
        Action::Config => connection.print(&json!({
            "server": connection.server,
            "authentication": connection.api_key.is_some(),
            "output": connection.output,
            "dryRun": connection.dry_run,
        })),
        Action::Watch { id, interval_secs } => watch(&connection, &id, interval_secs).await,
        Action::Command(invocation) => {
            if let Some(prompt) = confirmation(&invocation.command) {
                connection.confirm(prompt)?;
            }
            connection.execute(invocation).await
        }
        Action::Shell => Err("the shell is already running".into()),
    }
}

fn confirmation(command: &Command) -> Option<&'static str> {
    match command {
        Command::Rdf(RdfCommand::DeleteInstance { .. }) => Some("Delete this instance?"),
        Command::Rdf(RdfCommand::DeleteStatements { .. }) => Some("Delete these statements?"),
        Command::Rdf(RdfCommand::ClearGraph { .. }) => Some("Clear this graph?"),
        Command::Ontology(OntologyCommand::DeleteClass { .. }) => Some("Delete this class?"),
        Command::Ontology(OntologyCommand::DeleteProperty { .. }) => Some("Delete this property?"),
        Command::Ontology(OntologyCommand::DeleteOntology { .. }) => {
            Some("Delete this ontology and its versions?")
        }
        Command::Taxonomy(
            TaxonomyCommand::DeleteScheme { .. }
            | TaxonomyCommand::DeleteConcept { .. }
            | TaxonomyCommand::DeleteCollection { .. }
            | TaxonomyCommand::MergeSchemes { .. },
        ) => Some("Apply this destructive taxonomy change?"),
        Command::External(ExternalCommand::Delete { .. }) => Some("Delete this external ontology?"),
        Command::Semantic(SemanticCommand::DeleteEmbeddings { .. }) => {
            Some("Delete these embeddings?")
        }
        _ => None,
    }
}

async fn watch(connection: &Connection, id: &Id, interval_secs: u64) -> CliResult<()> {
    loop {
        let value = connection
            .send(&Command::Job(JobCommand::Get { id: id.clone() }))
            .await?;
        let state = value
            .get("state")
            .and_then(|state| state.as_str())
            .unwrap_or("unknown");
        let progress = value
            .get("progress")
            .and_then(|progress| progress.as_f64())
            .unwrap_or(0.0);
        eprintln!("{id} {state} {:.0}%", progress * 100.0);
        if matches!(state, "completed" | "failed" | "cancelled") {
            return connection.print(&value);
        }
        tokio::time::sleep(Duration::from_secs(interval_secs.max(1))).await;
    }
}

async fn shell(connection: Connection) -> CliResult<()> {
    println!(
        "onto shell {} — type `help` for commands, `quit` to exit",
        env!("CARGO_PKG_VERSION")
    );
    let mut history = load_history();
    loop {
        let Some(line) = read_line("onto> ")? else {
            break;
        };
        let line = line.trim().to_owned();
        if line.is_empty() {
            continue;
        }
        let mut line = line;
        while line.ends_with('\\') {
            line.pop();
            let Some(next) = read_line("   ...> ")? else {
                break;
            };
            line.push(' ');
            line.push_str(next.trim());
        }
        if matches!(line.as_str(), "quit" | "exit" | ":q") {
            break;
        }
        if line == "help" {
            println!("{}", Cli::command().render_help());
            continue;
        }
        if line == "history" {
            for (index, entry) in history.iter().enumerate() {
                println!("{:>4}  {entry}", index + 1);
            }
            continue;
        }
        history.push(line.clone());
        append_history(&line);
        let Some(tokens) = shlex::split(&line) else {
            eprintln!("onto: unbalanced quotes");
            continue;
        };
        let parsed = match Cli::try_parse_from(std::iter::once("onto".to_owned()).chain(tokens)) {
            Ok(parsed) => parsed,
            Err(error) => {
                println!("{error}");
                continue;
            }
        };
        match cli::action(parsed.command) {
            Ok(Action::Shell) => eprintln!("onto: already in the shell"),
            Ok(Action::Server(_)) => eprintln!("onto: start the server from a separate process"),
            Ok(action) => {
                if let Err(error) = perform(connection.clone(), action).await {
                    eprintln!("onto: {error}");
                }
            }
            Err(error) => eprintln!("onto: {error}"),
        }
    }
    Ok(())
}

fn read_line(prompt: &str) -> CliResult<Option<String>> {
    print!("{prompt}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line)?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(line))
}

fn history_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".onto_history"))
}

fn load_history() -> Vec<String> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .map(|document| {
            document
                .lines()
                .filter(|line| !line.trim().is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn append_history(line: &str) {
    let Some(path) = history_path() else {
        return;
    };
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

/// Keeps the terminal helper import used on platforms without a TTY.
#[allow(dead_code)]
fn tty() -> bool {
    std::io::stdin().is_terminal()
}
