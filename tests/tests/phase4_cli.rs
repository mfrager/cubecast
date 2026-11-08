//! Phase 4 CLI parsing and command-mapping tests.

use clap::Parser;
use ontology_cli::cli::{self, Action, Cli};
use ontology_core::command::{Command, JobCommand, OntologyCommand, RdfCommand, SemanticCommand};
use ontology_core::job::JobRequest;
use ontology_core::semantic::ClassificationStrategy;

fn parse_action(args: &[&str]) -> Action {
    let cli = Cli::try_parse_from(args).expect("parse CLI");
    cli::action(cli.command).expect("build action")
}

#[test]
fn parses_instance_commands() {
    let action = parse_action(&["onto", "instance", "get", "urn:example:cli-1"]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Rdf(RdfCommand::GetInstance { id, graph }) => {
                assert_eq!(id.as_str(), "urn:example:cli-1");
                assert!(graph.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }

    let payload = r#"{"id":"urn:example:cli-2","types":[],"properties":{}}"#;
    let action = parse_action(&["onto", "instance", "create", "--json", payload]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Rdf(RdfCommand::CreateInstance(new)) => {
                assert_eq!(new.id.as_str(), "urn:example:cli-2");
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_classification_options() {
    let action = parse_action(&[
        "onto",
        "classify",
        "run",
        "urn:example:cli-3",
        "--threshold",
        "0.7",
        "--strategy",
        "exact",
        "--no-persist",
    ]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Semantic(SemanticCommand::Classify { instance, options }) => {
                assert_eq!(instance.as_str(), "urn:example:cli-3");
                assert!((options.threshold - 0.7).abs() < f64::EPSILON);
                assert_eq!(options.strategies, vec![ClassificationStrategy::Exact]);
                assert!(!options.persist);
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_ontology_updates_and_jobs() {
    let payload = r#"{"id":"urn:example:ignored","labels":{},"comments":{},"imports":[]}"#;
    let action = parse_action(&[
        "onto",
        "ontology",
        "update",
        "urn:example:ontology",
        "--json",
        payload,
    ]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Ontology(OntologyCommand::UpdateOntology(ontology)) => {
                assert_eq!(ontology.id.as_str(), "urn:example:ontology");
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }

    let action = parse_action(&["onto", "job", "get", "urn:onto:job:cli"]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Job(JobCommand::Get { id }) => {
                assert_eq!(id.as_str(), "urn:onto:job:cli");
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }

    assert!(matches!(
        parse_action(&[
            "onto",
            "job",
            "watch",
            "urn:onto:job:cli",
            "--interval-secs",
            "2"
        ]),
        Action::Watch {
            interval_secs: 2,
            ..
        }
    ));

    let action = parse_action(&[
        "onto",
        "job",
        "import",
        "<urn:example:s> <urn:example:p> \"v\" .",
        "--format",
        "ntriples",
    ]);
    match action {
        Action::Command(invocation) => match invocation.command {
            Command::Job(JobCommand::Submit(JobRequest::RdfImport { format, data, .. })) => {
                assert_eq!(format, "ntriples");
                assert!(data.contains("urn:example:s"));
            }
            other => panic!("unexpected command: {other:?}"),
        },
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_global_flags_and_shell() {
    let cli = Cli::try_parse_from(["onto", "--dry-run", "--output", "yaml", "rdf", "graphs"])
        .expect("parse CLI");
    assert!(cli.dry_run);
    assert_eq!(cli.output, Some(ontology_cli::OutputFormat::Yaml));
    match cli::action(cli.command).expect("build action") {
        Action::Command(invocation) => {
            assert_eq!(invocation.command, Command::Rdf(RdfCommand::ListGraphs));
        }
        other => panic!("unexpected action: {other:?}"),
    }

    assert!(matches!(parse_action(&["onto", "shell"]), Action::Shell));
    assert!(matches!(parse_action(&["onto", "config"]), Action::Config));
}
