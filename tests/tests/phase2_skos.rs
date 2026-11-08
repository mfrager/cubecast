//! Phase 2.2 and 2.3 tests: concept schemes, concepts, collections,
//! hierarchical and associative relations, search, movement, merging, and
//! taxonomy validation.

use ontology_application::ApplicationService;
use ontology_core::command::{Command, CommandResult, TaxonomyCommand};
use ontology_core::error::Error;
use ontology_core::rdf::OntologyService;
use ontology_core::skos::{
    Concept, ConceptCollection, ConceptScheme, LabelKind, SkosRelation, TaxonomyManager,
    TaxonomyQuery,
};
use ontology_integration_tests::{ex, memory_store_handle};
use ontology_skos::SkosManager;

fn manager() -> SkosManager {
    SkosManager::new(memory_store_handle())
}

#[tokio::test]
async fn manages_concept_schemes() {
    let manager = manager();
    let scheme = ex("animals");

    manager
        .create_scheme(
            ConceptScheme::new(scheme.clone())
                .with_label("en", "Animals")
                .with_definition("en", "An animal taxonomy")
                .with_version("1.0"),
        )
        .await
        .expect("create scheme");

    let loaded = manager.get_scheme(&scheme).await.expect("get scheme");
    assert_eq!(loaded.labels.get("en").map(String::as_str), Some("Animals"));
    assert_eq!(loaded.version.as_deref(), Some("1.0"));

    assert!(matches!(
        manager
            .create_scheme(ConceptScheme::new(scheme.clone()))
            .await,
        Err(Error::Conflict { .. })
    ));

    manager
        .update_scheme(
            ConceptScheme::new(scheme.clone())
                .with_label("en", "Animal taxonomy")
                .with_version("1.1"),
        )
        .await
        .expect("update scheme");
    let updated = manager.get_scheme(&scheme).await.expect("get scheme");
    assert_eq!(updated.version.as_deref(), Some("1.1"));
    assert_eq!(
        manager
            .scheme_versions(&scheme)
            .await
            .expect("versions")
            .len(),
        2
    );
    assert_eq!(manager.list_schemes().await.expect("list").len(), 1);

    let affected = manager.delete_scheme(&scheme).await.expect("delete scheme");
    assert!(affected > 0);
    assert!(matches!(
        manager.get_scheme(&scheme).await,
        Err(Error::NotFound { .. })
    ));
}

#[tokio::test]
async fn manages_concepts_and_labels() {
    let manager = manager();
    let scheme = ex("scheme");
    manager
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .expect("create scheme");

    let dog = ex("dog");
    manager
        .create_concept(
            Concept::new(dog.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Dog", "en")
                .with_alt_label("Doggo", "en")
                .with_hidden_label("Canis familiaris", "en")
                .with_definition("A domesticated canid", "en")
                .with_notation("D1"),
        )
        .await
        .expect("create concept");

    let loaded = manager.get_concept(&dog).await.expect("get concept");
    assert_eq!(loaded.scheme, Some(scheme.clone()));
    assert_eq!(loaded.pref_labels.len(), 1);
    assert_eq!(loaded.pref_labels[0].value, "Dog");
    assert_eq!(loaded.pref_labels[0].language.as_deref(), Some("en"));
    assert_eq!(loaded.alt_labels.len(), 1);
    assert_eq!(loaded.hidden_labels.len(), 1);
    assert_eq!(loaded.notations, vec!["D1"]);

    // Duplicate preferred labels are rejected per scheme, whitespace and case
    // insensitive.
    assert!(matches!(
        manager
            .create_concept(
                Concept::new(ex("dog2"))
                    .with_scheme(scheme.clone())
                    .with_pref_label(" dog ", "en"),
            )
            .await,
        Err(Error::Conflict { .. })
    ));

    let listed = manager
        .list_concepts(
            TaxonomyQuery::new()
                .with_scheme(scheme.clone())
                .with_label("doggo"),
        )
        .await
        .expect("list concepts");
    assert_eq!(listed.len(), 1);
    assert!(manager
        .list_concepts(TaxonomyQuery::new().with_label("missing"))
        .await
        .expect("list concepts")
        .is_empty());
}

#[tokio::test]
async fn searches_labels_and_notations() {
    let manager = manager();
    let scheme = ex("scheme");
    manager
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(ex("dog"))
                .with_scheme(scheme.clone())
                .with_pref_label("Dog", "en")
                .with_alt_label("Doggo", "en")
                .with_hidden_label("Canis familiaris", "en")
                .with_notation("D1"),
        )
        .await
        .unwrap();

    let matches = manager.search("dog", None).await.expect("search");
    assert!(matches
        .iter()
        .any(|hit| hit.kind == LabelKind::Preferred && hit.label == "Dog"));
    assert!(matches.iter().any(|hit| hit.kind == LabelKind::Alternative));
    assert_eq!(matches[0].score, 1.0);
    assert!(matches.iter().all(|hit| hit.concept == ex("dog")));

    let hidden = manager.search("canis", None).await.expect("search");
    assert!(hidden.iter().any(|hit| hit.kind == LabelKind::Hidden));
    assert!(manager
        .search("  ", None)
        .await
        .expect("empty search")
        .is_empty());
    assert!(manager
        .search("dog", Some(&ex("other")))
        .await
        .expect("scoped search")
        .is_empty());
}

#[tokio::test]
async fn builds_and_edits_hierarchies() {
    let manager = manager();
    let scheme = ex("scheme");
    manager
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .unwrap();

    let animal = ex("animal");
    let dog = ex("dog");
    let poodle = ex("poodle");
    let cat = ex("cat");
    manager
        .create_concept(
            Concept::new(animal.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Animal", "en"),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(dog.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Dog", "en")
                .with_broader(animal.clone()),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(poodle.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Poodle", "en")
                .with_broader(dog.clone()),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(cat.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Cat", "en"),
        )
        .await
        .unwrap();

    manager
        .add_relation(&cat, SkosRelation::Broader, &animal)
        .await
        .expect("add broader");
    let loaded = manager.get_concept(&animal).await.expect("get animal");
    assert!(loaded.narrower.contains(&dog));
    assert!(loaded.narrower.contains(&cat));

    manager
        .add_relation(&cat, SkosRelation::Related, &dog)
        .await
        .expect("add related");
    let cat_loaded = manager.get_concept(&cat).await.expect("get cat");
    assert_same_ids(&cat_loaded.related, &[dog.clone()]);
    assert!(manager
        .get_concept(&dog)
        .await
        .expect("get dog")
        .related
        .contains(&cat));

    let relations = manager.relations(&cat).await.expect("relations");
    assert!(relations
        .iter()
        .any(|relation| relation.relation == SkosRelation::Broader && relation.target == animal));
    assert!(relations
        .iter()
        .any(|relation| relation.relation == SkosRelation::Related && relation.target == dog));

    manager
        .remove_relation(&cat, SkosRelation::Broader, &animal)
        .await
        .expect("remove broader");
    assert!(!manager
        .get_concept(&animal)
        .await
        .expect("get animal")
        .narrower
        .contains(&cat));

    // Cycles and self-parenting are rejected.
    assert!(matches!(
        manager
            .add_relation(&animal, SkosRelation::Broader, &poodle)
            .await,
        Err(Error::Validation { .. })
    ));
    assert!(matches!(
        manager
            .add_relation(&cat, SkosRelation::Broader, &cat)
            .await,
        Err(Error::Validation { .. })
    ));

    let descendants = manager.descendants(&animal).await.expect("descendants");
    assert!(descendants.iter().any(|concept| concept.id == dog));
    assert!(descendants.iter().any(|concept| concept.id == poodle));

    let ancestors = manager.ancestors(&poodle).await.expect("ancestors");
    assert_eq!(ancestors.len(), 2);
    assert!(ancestors.iter().any(|concept| concept.id == animal));

    let trees = manager.tree(&scheme).await.expect("tree");
    let root = trees
        .iter()
        .find(|tree| tree.concept == animal)
        .expect("animal root");
    assert_eq!(root.label.as_deref(), Some("Animal"));
    let dog_node = root
        .children
        .iter()
        .find(|child| child.concept == dog)
        .expect("dog child");
    assert!(dog_node
        .children
        .iter()
        .any(|child| child.concept == poodle));
}

fn assert_same_ids(left: &[ontology_core::id::Id], right: &[ontology_core::id::Id]) {
    assert_eq!(left.len(), right.len());
    for value in right {
        assert!(left.contains(value), "missing {value}");
    }
}

#[tokio::test]
async fn moves_concepts_and_validates() {
    let manager = manager();
    let scheme = ex("scheme");
    let other = ex("other-scheme");
    manager
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .unwrap();
    manager
        .create_scheme(ConceptScheme::new(other.clone()))
        .await
        .unwrap();

    let parent = ex("parent");
    let child = ex("child");
    let moving = ex("moving");
    manager
        .create_concept(
            Concept::new(parent.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Parent", "en"),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(child.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Child", "en")
                .with_broader(parent.clone()),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(moving.clone())
                .with_scheme(scheme.clone())
                .with_pref_label("Moving", "en"),
        )
        .await
        .unwrap();

    manager
        .move_concept(&moving, Some(&child), None)
        .await
        .expect("move under child");
    assert_eq!(
        manager.get_concept(&moving).await.unwrap().broader,
        vec![child.clone()]
    );

    manager
        .move_concept(&moving, None, Some(&other))
        .await
        .expect("move to other scheme");
    let moved = manager.get_concept(&moving).await.unwrap();
    assert_eq!(moved.scheme, Some(other));
    assert!(moved.broader.is_empty());

    manager
        .create_concept(Concept::new(ex("no-label")).with_scheme(scheme.clone()))
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(ex("dangling"))
                .with_scheme(scheme.clone())
                .with_pref_label("Dangling", "en")
                .with_broader(ex("ghost")),
        )
        .await
        .unwrap();
    manager
        .add_relation(&parent, SkosRelation::Related, &child)
        .await
        .expect("related");

    let report = manager
        .validate(Some(&scheme))
        .await
        .expect("validate taxonomy");
    assert!(report.checked_concepts >= 3);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "missing-pref-label"));
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "unknown-broader"));
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "related-overlaps-hierarchy"));
    assert!(report.is_valid());
}

#[tokio::test]
async fn merges_schemes() {
    let manager = manager();
    let source = ex("source");
    let target = ex("target");
    manager
        .create_scheme(ConceptScheme::new(source.clone()).with_label("en", "Source"))
        .await
        .unwrap();
    manager
        .create_scheme(ConceptScheme::new(target.clone()).with_label("en", "Target"))
        .await
        .unwrap();

    let shared = ex("shared");
    manager
        .create_concept(
            Concept::new(shared.clone())
                .with_scheme(source.clone())
                .with_pref_label("Shared", "en"),
        )
        .await
        .unwrap();
    manager
        .create_concept(
            Concept::new(ex("existing"))
                .with_scheme(target.clone())
                .with_pref_label("Shared", "en"),
        )
        .await
        .unwrap();
    manager
        .update_scheme(
            ConceptScheme::new(source.clone())
                .with_label("en", "Source")
                .with_top_concept(shared.clone()),
        )
        .await
        .unwrap();

    let report = manager
        .merge_schemes(&source, &target)
        .await
        .expect("merge schemes");
    assert_eq!(report.checked_concepts, 1);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "duplicate-label"));

    assert_eq!(
        manager.get_concept(&shared).await.unwrap().scheme,
        Some(target.clone())
    );
    assert!(matches!(
        manager.get_scheme(&source).await,
        Err(Error::NotFound { .. })
    ));
    assert!(manager
        .get_scheme(&target)
        .await
        .expect("target")
        .top_concepts
        .contains(&shared));
}

#[tokio::test]
async fn manages_collections() {
    let manager = manager();
    let scheme = ex("scheme");
    manager
        .create_scheme(ConceptScheme::new(scheme.clone()))
        .await
        .unwrap();
    let collection = ex("edibles");
    manager
        .create_collection(
            ConceptCollection::new(collection.clone())
                .with_scheme(scheme.clone())
                .with_label("en", "Edibles")
                .with_member(ex("apple"))
                .with_member(ex("pear")),
        )
        .await
        .expect("create collection");

    let loaded = manager.get_collection(&collection).await.expect("get");
    assert_eq!(loaded.members.len(), 2);
    assert_eq!(loaded.labels.get("en").map(String::as_str), Some("Edibles"));
    assert_eq!(
        manager
            .list_collections(Some(&scheme))
            .await
            .expect("list")
            .len(),
        1
    );
    assert!(manager
        .list_collections(Some(&ex("other")))
        .await
        .expect("list")
        .is_empty());

    let affected = manager
        .delete_collection(&collection)
        .await
        .expect("delete collection");
    assert!(affected > 0);
    assert!(matches!(
        manager.get_collection(&collection).await,
        Err(Error::NotFound { .. })
    ));
}

#[tokio::test]
async fn dispatches_taxonomy_commands() {
    let service = ApplicationService::new(memory_store_handle());
    let scheme = ex("commands");

    let created = service
        .execute(Command::Taxonomy(TaxonomyCommand::CreateScheme(
            ConceptScheme::new(scheme.clone()).with_label("en", "Commands"),
        )))
        .await
        .expect("create scheme command");
    match created {
        CommandResult::Resource(resource) => {
            assert_eq!(resource.kind, "concept_scheme");
            assert_eq!(resource.id, scheme.as_str());
        }
        other => panic!("unexpected result: {other:?}"),
    }

    service
        .execute(Command::Taxonomy(TaxonomyCommand::CreateConcept(
            Concept::new(ex("widget"))
                .with_scheme(scheme.clone())
                .with_pref_label("Widget", "en"),
        )))
        .await
        .expect("create concept command");

    let listed = service
        .execute(Command::Taxonomy(TaxonomyCommand::ListConcepts(
            TaxonomyQuery::new(),
        )))
        .await
        .expect("list command");
    match listed {
        CommandResult::Collection(collection) => assert_eq!(collection.count, 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let searched = service
        .execute(Command::Taxonomy(TaxonomyCommand::SearchConcepts {
            query: "wid".to_owned(),
            scheme: Some(scheme.clone()),
        }))
        .await
        .expect("search command");
    match searched {
        CommandResult::Collection(collection) => assert_eq!(collection.count, 1),
        other => panic!("unexpected result: {other:?}"),
    }

    let tree = service
        .execute(Command::Taxonomy(TaxonomyCommand::ConceptTree {
            scheme: scheme.clone(),
        }))
        .await
        .expect("tree command");
    match tree {
        CommandResult::Resource(resource) => assert_eq!(resource.kind, "concept_tree"),
        other => panic!("unexpected result: {other:?}"),
    }

    let validated = service
        .execute(Command::Taxonomy(TaxonomyCommand::ValidateTaxonomy {
            scheme: Some(scheme.clone()),
        }))
        .await
        .expect("validate command");
    assert!(matches!(validated, CommandResult::Taxonomy(_)));

    let deleted = service
        .execute(Command::Taxonomy(TaxonomyCommand::DeleteConcept {
            id: ex("widget"),
        }))
        .await
        .expect("delete concept command");
    match deleted {
        CommandResult::Report(report) => assert!(report.affected >= 1),
        other => panic!("unexpected result: {other:?}"),
    }
}
