//! SPARQL rewriting: one host evaluates every query, satellites are reached
//! through in-process `SERVICE <oxigraph://…>` clauses.

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::Arc;

use ontology_core::error::{Error, Result};
use ontology_core::id::Id;
use serde::Serialize;
use spargebra::algebra::{Expression, GraphPattern};
use spargebra::term::{NamedNode, NamedNodePattern, Variable};
use spargebra::{Query, SparqlParser};

use crate::catalog::{CatalogSnapshot, Generation, GraphRoute, GraphStoreId, StoreLayout};

/// Why a plan does not use the fused path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FallbackReason {
    /// The satellite uses layout `named`, so `SERVICE` keeps its boundary.
    NamedLayoutService {
        /// Owning store.
        store: String,
    },
    /// `GRAPH ?g` was expanded into a union over all registered graphs.
    VariableGraphUnion,
    /// The satellite is unavailable, so its branch yields no solutions.
    DegradedStore {
        /// Owning store.
        store: String,
    },
    /// A `GRAPH` clause named an unregistered graph.
    UnknownGraph {
        /// The graph IRI.
        graph: String,
    },
}

/// A planned query.
#[derive(Debug)]
pub struct QueryPlan {
    /// Store whose engine evaluates the query.
    pub host: GraphStoreId,
    /// Rewritten query.
    pub query: Query,
    /// Graph-to-store routes used by the plan.
    pub routes: Vec<GraphRoute>,
    /// Fallbacks taken.
    pub fallbacks: Vec<FallbackReason>,
    /// Graphs the query references.
    pub referenced: BTreeSet<Id>,
}

/// Bounded cache keyed by query text and generation.
#[derive(Debug)]
pub(crate) struct PlanCache {
    generation: Generation,
    entries: HashMap<String, Arc<QueryPlan>>,
    order: VecDeque<String>,
    capacity: usize,
}

impl PlanCache {
    pub(crate) fn new(generation: Generation) -> Self {
        Self {
            generation,
            entries: HashMap::new(),
            order: VecDeque::new(),
            capacity: 512,
        }
    }

    pub(crate) fn get(&mut self, text: &str, generation: Generation) -> Option<Arc<QueryPlan>> {
        if self.generation != generation {
            self.entries.clear();
            self.order.clear();
            self.generation = generation;
        }
        self.entries.get(text).cloned()
    }

    pub(crate) fn insert(&mut self, text: String, plan: Arc<QueryPlan>) {
        if self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.order.push_back(text.clone());
        self.entries.insert(text, plan);
    }
}

#[derive(Default)]
struct Analysis {
    concrete: BTreeSet<Id>,
    variable: bool,
}

fn analyse_query(query: &Query) -> Analysis {
    let pattern = match query {
        Query::Select(query) => &query.pattern,
        Query::Construct(query) => &query.pattern,
        Query::Describe(query) => &query.pattern,
        Query::Ask(query) => &query.pattern,
    };
    let mut analysis = Analysis::default();
    analyse_pattern(pattern, &mut analysis);
    analysis
}

fn analyse_pattern(pattern: &GraphPattern, analysis: &mut Analysis) {
    match pattern {
        GraphPattern::Graph { name, inner } => {
            match name {
                NamedNodePattern::NamedNode(node) => {
                    if let Ok(graph) = Id::new(node.as_str()) {
                        analysis.concrete.insert(graph);
                    }
                }
                NamedNodePattern::Variable(_) => analysis.variable = true,
            }
            analyse_pattern(inner, analysis);
        }
        GraphPattern::Join { left, right }
        | GraphPattern::Union { left, right }
        | GraphPattern::Minus { left, right }
        | GraphPattern::Lateral { left, right } => {
            analyse_pattern(left, analysis);
            analyse_pattern(right, analysis);
        }
        GraphPattern::LeftJoin { left, right, .. } => {
            analyse_pattern(left, analysis);
            analyse_pattern(right, analysis);
        }
        GraphPattern::Filter { inner, .. }
        | GraphPattern::Extend { inner, .. }
        | GraphPattern::OrderBy { inner, .. }
        | GraphPattern::Project { inner, .. }
        | GraphPattern::Distinct { inner }
        | GraphPattern::Reduced { inner }
        | GraphPattern::Slice { inner, .. }
        | GraphPattern::Group { inner, .. } => analyse_pattern(inner, analysis),
        GraphPattern::Bgp { .. } | GraphPattern::Path { .. } | GraphPattern::Values { .. } => {}
        // User-written SERVICE clauses are never rewritten.
        GraphPattern::Service { .. } => {}
    }
}

fn elect_host(snapshot: &CatalogSnapshot, referenced: &BTreeSet<Id>) -> GraphStoreId {
    if referenced.is_empty() {
        return snapshot.main.id.clone();
    }
    let mut counts: Vec<(GraphStoreId, usize, i64)> = Vec::new();
    for graph in referenced {
        let Ok(handle) = snapshot.resolve_graph(graph) else {
            continue;
        };
        if !handle.is_ready() {
            continue;
        }
        match counts.iter_mut().find(|(id, _, _)| *id == handle.id) {
            Some((_, count, _)) => *count += 1,
            None => counts.push((handle.id.clone(), 1, handle.mount_order)),
        }
    }
    if counts.iter().any(|(id, _, _)| *id == snapshot.main.id) {
        return snapshot.main.id.clone();
    }
    counts
        .into_iter()
        .min_by(|left, right| right.1.cmp(&left.1).then_with(|| left.2.cmp(&right.2)))
        .map_or_else(|| snapshot.main.id.clone(), |(id, _, _)| id)
}

/// Plans a query against `snapshot`.
///
/// # Errors
///
/// Returns [`Error::Query`] for invalid SPARQL and [`Error::Unsupported`] when
/// the query uses a dataset clause, which would silently deselect satellites.
pub fn plan_query(snapshot: &CatalogSnapshot, text: &str) -> Result<QueryPlan> {
    let query = SparqlParser::new()
        .parse_query(text)
        .map_err(Error::query)?;
    if query.dataset().is_some() {
        return Err(Error::Unsupported {
            operation: "query dataset clause (FROM / FROM NAMED); select graphs with GRAPH instead"
                .to_owned(),
        });
    }
    let analysis = analyse_query(&query);
    for graph in &analysis.concrete {
        snapshot.resolve_graph(graph)?;
    }
    let mut referenced = analysis.concrete;
    if analysis.variable {
        referenced.extend(snapshot.enabled_graphs());
    }
    let host = elect_host(snapshot, &referenced);
    let mut fallbacks = Vec::new();
    let query = rewrite_query(snapshot, &host, query, &mut fallbacks);
    let routes = referenced
        .iter()
        .filter_map(|graph| {
            snapshot.resolve_graph(graph).ok().map(|handle| GraphRoute {
                graph: graph.clone(),
                store: handle.id.clone(),
            })
        })
        .collect();
    Ok(QueryPlan {
        host,
        query,
        routes,
        fallbacks,
        referenced,
    })
}

fn rewrite_query(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    query: Query,
    fallbacks: &mut Vec<FallbackReason>,
) -> Query {
    match query {
        Query::Select(mut query) => {
            query.pattern = rewrite_pattern(snapshot, host, query.pattern, fallbacks);
            Query::Select(query)
        }
        Query::Construct(mut query) => {
            query.pattern = rewrite_pattern(snapshot, host, query.pattern, fallbacks);
            Query::Construct(query)
        }
        Query::Describe(mut query) => {
            query.pattern = rewrite_pattern(snapshot, host, query.pattern, fallbacks);
            Query::Describe(query)
        }
        Query::Ask(mut query) => {
            query.pattern = rewrite_pattern(snapshot, host, query.pattern, fallbacks);
            Query::Ask(query)
        }
    }
}

/// Rewrites a graph pattern so every satellite is reached through `SERVICE`.
pub(crate) fn rewrite_pattern(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    pattern: GraphPattern,
    fallbacks: &mut Vec<FallbackReason>,
) -> GraphPattern {
    match pattern {
        GraphPattern::Graph { name, inner } => match name {
            NamedNodePattern::NamedNode(node) => match Id::new(node.as_str()) {
                Ok(graph) => rewrite_graph(snapshot, host, &graph, node, *inner, fallbacks),
                Err(_) => GraphPattern::Graph {
                    name: NamedNodePattern::NamedNode(node),
                    inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
                },
            },
            NamedNodePattern::Variable(variable) => {
                expand_variable_graph(snapshot, host, variable, *inner, fallbacks)
            }
        },
        GraphPattern::Join { left, right } => GraphPattern::Join {
            left: Box::new(rewrite_pattern(snapshot, host, *left, fallbacks)),
            right: Box::new(rewrite_pattern(snapshot, host, *right, fallbacks)),
        },
        GraphPattern::LeftJoin {
            left,
            right,
            expression,
        } => GraphPattern::LeftJoin {
            left: Box::new(rewrite_pattern(snapshot, host, *left, fallbacks)),
            right: Box::new(rewrite_pattern(snapshot, host, *right, fallbacks)),
            expression,
        },
        GraphPattern::Lateral { left, right } => GraphPattern::Lateral {
            left: Box::new(rewrite_pattern(snapshot, host, *left, fallbacks)),
            right: Box::new(rewrite_pattern(snapshot, host, *right, fallbacks)),
        },
        GraphPattern::Union { left, right } => GraphPattern::Union {
            left: Box::new(rewrite_pattern(snapshot, host, *left, fallbacks)),
            right: Box::new(rewrite_pattern(snapshot, host, *right, fallbacks)),
        },
        GraphPattern::Minus { left, right } => GraphPattern::Minus {
            left: Box::new(rewrite_pattern(snapshot, host, *left, fallbacks)),
            right: Box::new(rewrite_pattern(snapshot, host, *right, fallbacks)),
        },
        GraphPattern::Filter { expr, inner } => GraphPattern::Filter {
            expr: rewrite_expression(snapshot, host, expr, fallbacks),
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
        },
        GraphPattern::Extend {
            inner,
            variable,
            expression,
        } => GraphPattern::Extend {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
            variable,
            expression: rewrite_expression(snapshot, host, expression, fallbacks),
        },
        GraphPattern::OrderBy { inner, expression } => GraphPattern::OrderBy {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
            expression,
        },
        GraphPattern::Project { inner, variables } => GraphPattern::Project {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
            variables,
        },
        GraphPattern::Distinct { inner } => GraphPattern::Distinct {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
        },
        GraphPattern::Reduced { inner } => GraphPattern::Reduced {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
        },
        GraphPattern::Slice {
            inner,
            start,
            length,
        } => GraphPattern::Slice {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
            start,
            length,
        },
        GraphPattern::Group {
            inner,
            variables,
            aggregates,
        } => GraphPattern::Group {
            inner: Box::new(rewrite_pattern(snapshot, host, *inner, fallbacks)),
            variables,
            aggregates,
        },
        other => other,
    }
}

fn rewrite_expression(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    expression: Expression,
    fallbacks: &mut Vec<FallbackReason>,
) -> Expression {
    match expression {
        Expression::Or(left, right) => Expression::Or(
            Box::new(rewrite_expression(snapshot, host, *left, fallbacks)),
            Box::new(rewrite_expression(snapshot, host, *right, fallbacks)),
        ),
        Expression::And(left, right) => Expression::And(
            Box::new(rewrite_expression(snapshot, host, *left, fallbacks)),
            Box::new(rewrite_expression(snapshot, host, *right, fallbacks)),
        ),
        Expression::In(value, list) => Expression::In(
            Box::new(rewrite_expression(snapshot, host, *value, fallbacks)),
            list.into_iter()
                .map(|item| rewrite_expression(snapshot, host, item, fallbacks))
                .collect(),
        ),
        Expression::Exists(pattern) => Expression::Exists(Box::new(rewrite_pattern(
            snapshot, host, *pattern, fallbacks,
        ))),
        Expression::If(condition, then, otherwise) => Expression::If(
            Box::new(rewrite_expression(snapshot, host, *condition, fallbacks)),
            Box::new(rewrite_expression(snapshot, host, *then, fallbacks)),
            Box::new(rewrite_expression(snapshot, host, *otherwise, fallbacks)),
        ),
        Expression::Coalesce(items) => Expression::Coalesce(
            items
                .into_iter()
                .map(|item| rewrite_expression(snapshot, host, item, fallbacks))
                .collect(),
        ),
        Expression::FunctionCall(name, arguments) => Expression::FunctionCall(
            name,
            arguments
                .into_iter()
                .map(|argument| rewrite_expression(snapshot, host, argument, fallbacks))
                .collect(),
        ),
        other => other,
    }
}

fn rewrite_graph(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    graph: &Id,
    node: NamedNode,
    inner: GraphPattern,
    fallbacks: &mut Vec<FallbackReason>,
) -> GraphPattern {
    let inner = rewrite_pattern(snapshot, host, inner, fallbacks);
    graph_branch(snapshot, host, graph, node, inner, fallbacks)
}

fn graph_branch(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    graph: &Id,
    node: NamedNode,
    inner: GraphPattern,
    fallbacks: &mut Vec<FallbackReason>,
) -> GraphPattern {
    let Ok(handle) = snapshot.resolve_graph(graph) else {
        fallbacks.push(FallbackReason::UnknownGraph {
            graph: graph.to_string(),
        });
        return GraphPattern::Graph {
            name: NamedNodePattern::NamedNode(node),
            inner: Box::new(inner),
        };
    };
    if handle.id == *host {
        // A `single`-layout store keeps its data in the unnamed graph, so the
        // host reads it without a `GRAPH` wrapper.
        return match handle.layout {
            StoreLayout::Single => inner,
            StoreLayout::Named => GraphPattern::Graph {
                name: NamedNodePattern::NamedNode(node),
                inner: Box::new(inner),
            },
        };
    }
    if !handle.is_ready() {
        fallbacks.push(FallbackReason::DegradedStore {
            store: handle.id.to_string(),
        });
        return empty_pattern();
    }
    let service = NamedNode::new(snapshot.service_iri(&handle))
        .expect("store paths are validated as IRI-safe");
    match handle.layout {
        StoreLayout::Single => GraphPattern::Service {
            name: NamedNodePattern::NamedNode(service),
            inner: Box::new(inner),
            silent: false,
        },
        StoreLayout::Named => {
            fallbacks.push(FallbackReason::NamedLayoutService {
                store: handle.id.to_string(),
            });
            GraphPattern::Service {
                name: NamedNodePattern::NamedNode(service),
                inner: Box::new(GraphPattern::Graph {
                    name: NamedNodePattern::NamedNode(node),
                    inner: Box::new(inner),
                }),
                silent: false,
            }
        }
    }
}

fn expand_variable_graph(
    snapshot: &CatalogSnapshot,
    host: &GraphStoreId,
    variable: Variable,
    inner: GraphPattern,
    fallbacks: &mut Vec<FallbackReason>,
) -> GraphPattern {
    let graphs = snapshot.enabled_graphs();
    if graphs.is_empty() {
        return GraphPattern::Graph {
            name: NamedNodePattern::Variable(variable),
            inner: Box::new(inner),
        };
    }
    fallbacks.push(FallbackReason::VariableGraphUnion);
    let mut combined: Option<GraphPattern> = None;
    for graph in graphs {
        let node =
            NamedNode::new(graph.as_str().to_owned()).expect("registered graph IRIs are valid");
        let branch = graph_branch(
            snapshot,
            host,
            &graph,
            node.clone(),
            rewrite_pattern(snapshot, host, inner.clone(), fallbacks),
            fallbacks,
        );
        let bound = GraphPattern::Extend {
            inner: Box::new(branch),
            variable: variable.clone(),
            expression: Expression::NamedNode(node),
        };
        combined = Some(match combined {
            None => bound,
            Some(left) => GraphPattern::Union {
                left: Box::new(left),
                right: Box::new(bound),
            },
        });
    }
    combined.unwrap_or(GraphPattern::Graph {
        name: NamedNodePattern::Variable(variable),
        inner: Box::new(inner),
    })
}

fn empty_pattern() -> GraphPattern {
    GraphPattern::Filter {
        expr: Expression::Literal(false.into()),
        inner: Box::new(GraphPattern::Bgp {
            patterns: Vec::new(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::snapshot;

    fn plan(text: &str) -> QueryPlan {
        plan_query(&snapshot(), text).expect("plan")
    }

    #[test]
    fn keeps_host_graphs_and_services_named_layouts() {
        let plan = plan(&format!(
            "SELECT * WHERE {{ GRAPH <{}> {{ ?s ?a ?b }} GRAPH <urn:onto:data:products> {{ ?s ?p ?o }} }}",
            ontology_core::graph::GraphKind::Asserted.iri_str()
        ));
        assert_eq!(plan.host.as_str(), "urn:onto:store:main");
        let text = plan.query.to_string();
        assert!(text.contains("GRAPH <urn:onto:data:asserted>"));
        assert!(text.contains("SERVICE <oxigraph:///srv/onto/data/graphs/products>"));
        assert!(text.contains("GRAPH <urn:onto:data:products>"));
        assert!(plan
            .fallbacks
            .iter()
            .any(|fallback| matches!(fallback, FallbackReason::NamedLayoutService { .. })));
    }

    #[test]
    fn fuses_single_layout_satellites_without_an_inner_graph() {
        let plan = plan(&format!(
            "SELECT * WHERE {{ GRAPH <{}> {{ ?s ?a ?b }} GRAPH <urn:onto:external:dbpedia> {{ ?s ?p ?o }} }}",
            ontology_core::graph::GraphKind::Asserted.iri_str()
        ));
        assert_eq!(plan.host.as_str(), "urn:onto:store:main");
        assert!(plan.fallbacks.is_empty());
        let text = plan.query.to_string();
        assert!(text.contains("SERVICE <oxigraph:///srv/onto/data/graphs/dbpedia>"));
        assert!(!text.contains("GRAPH <urn:onto:external:dbpedia>"));
    }

    #[test]
    fn evaluates_single_store_queries_on_that_store() {
        let plan = plan("SELECT * WHERE { GRAPH <urn:onto:external:dbpedia> { ?s ?p ?o } }");
        assert_eq!(plan.host.as_str(), "urn:onto:store:dbpedia");
        assert!(plan.fallbacks.is_empty());
        assert!(!plan
            .query
            .to_string()
            .contains("GRAPH <urn:onto:external:dbpedia>"));
    }

    #[test]
    fn expands_variable_graphs_into_a_bound_union() {
        let plan = plan("SELECT ?g ?s WHERE { GRAPH ?g { ?s ?p ?o } }");
        let text = plan.query.to_string();
        assert!(text.contains("BIND("));
        assert!(text.contains("UNION"));
        assert!(plan
            .fallbacks
            .iter()
            .any(|fallback| matches!(fallback, FallbackReason::VariableGraphUnion)));
    }

    #[test]
    fn rewrites_graphs_inside_exists() {
        let plan = plan(&format!(
            "SELECT * WHERE {{ GRAPH <{}> {{ ?s ?p ?o }} FILTER EXISTS {{ GRAPH <urn:onto:external:dbpedia> {{ ?s ?p ?o }} }} }}",
            ontology_core::graph::GraphKind::Asserted.iri_str()
        ));
        assert!(plan
            .query
            .to_string()
            .contains("SERVICE <oxigraph:///srv/onto/data/graphs/dbpedia>"));
    }

    #[test]
    fn rejects_dataset_clauses() {
        let error = plan_query(
            &snapshot(),
            "SELECT * FROM <urn:onto:data:asserted> WHERE { ?s ?p ?o }",
        )
        .expect_err("dataset clause");
        assert!(matches!(error, Error::Unsupported { .. }));
    }

    #[test]
    fn rejects_unregistered_graphs() {
        let error = plan_query(
            &snapshot(),
            "SELECT * WHERE { GRAPH <urn:onto:data:missing> { ?s ?p ?o } }",
        )
        .expect_err("unknown graph");
        assert!(matches!(error, Error::GraphRegistry { .. }));
    }
}
