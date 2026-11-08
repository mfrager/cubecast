//! Phase 4 background job tests.

use std::time::Duration;

use ontology_application::ApplicationService;
use ontology_core::error::Error;
use ontology_core::job::{JobRequest, JobState, JobStatus};
use ontology_core::semantic::ClassificationOptions;
use ontology_integration_tests::{ex, memory_store_handle};

async fn wait(service: &ApplicationService, id: &ontology_core::id::Id) -> JobStatus {
    for _ in 0..400 {
        let status = service.jobs().get(id).await.expect("job status");
        if status.is_finished() {
            return status;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("job did not finish");
}

#[tokio::test]
async fn completes_and_reports_progress() {
    let service = ApplicationService::new(memory_store_handle());
    let status = service
        .jobs()
        .submit(JobRequest::ClassifyBatch {
            instances: Vec::new(),
            options: ClassificationOptions::default(),
        })
        .await
        .expect("submit job");
    assert_eq!(status.state, JobState::Queued);
    assert_eq!(status.total, 0);

    let status = wait(&service, &status.id).await;
    assert_eq!(status.state, JobState::Completed);
    assert_eq!(status.progress, 1.0);
    assert_eq!(status.processed, 0);
    assert!(status.result.is_some());
    assert!(status.error.is_none());

    let listed = service.jobs().list().await.expect("list jobs");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, status.id);
}

#[tokio::test]
async fn reports_failures_and_retries() {
    let service = ApplicationService::new(memory_store_handle());
    let status = service
        .jobs()
        .submit(JobRequest::EmbedBatch {
            instances: vec![ex("phase4-missing")],
            attributes: Vec::new(),
        })
        .await
        .expect("submit job");
    let failed = wait(&service, &status.id).await;
    assert_eq!(failed.state, JobState::Failed);
    assert!(failed.error.is_some());

    let retried = service.jobs().retry(&status.id).await.expect("retry");
    assert_eq!(retried.attempts, 1);
    assert!(!retried.state.is_terminal());
    let failed_again = wait(&service, &status.id).await;
    assert_eq!(failed_again.state, JobState::Failed);
    assert_eq!(failed_again.attempts, 1);
}

#[tokio::test]
async fn runs_rdf_import_and_export_jobs() {
    let service = ApplicationService::new(memory_store_handle());
    let turtle = "<https://example.org/phase4-job> <https://example.org/name> \"Job\" .";
    let status = service
        .jobs()
        .submit(JobRequest::RdfImport {
            format: "turtle".to_owned(),
            data: turtle.to_owned(),
            graph: None,
        })
        .await
        .expect("submit import job");
    let imported = wait(&service, &status.id).await;
    assert_eq!(imported.state, JobState::Completed);
    assert_eq!(imported.result.expect("import result")["imported"], 1);

    let status = service
        .jobs()
        .submit(JobRequest::RdfExport {
            format: "nquads".to_owned(),
            graph: None,
        })
        .await
        .expect("submit export job");
    let exported = wait(&service, &status.id).await;
    assert_eq!(exported.state, JobState::Completed);
    let result = exported.result.expect("export result");
    assert_eq!(result["format"], "nquads");
    assert!(result["data"]
        .as_str()
        .is_some_and(|data| data.contains("phase4-job")));
}

#[tokio::test]
async fn rejects_cancelling_finished_jobs() {
    let service = ApplicationService::new(memory_store_handle());
    let status = service
        .jobs()
        .submit(JobRequest::ClassifyBatch {
            instances: Vec::new(),
            options: ClassificationOptions::default(),
        })
        .await
        .expect("submit job");
    let finished = wait(&service, &status.id).await;
    assert!(finished.is_finished());

    let error = service
        .jobs()
        .cancel(&status.id)
        .await
        .expect_err("finished jobs cannot be cancelled");
    assert!(matches!(error, Error::Conflict { .. }));

    let missing = ontology_core::id::Id::new("urn:onto:job:missing").expect("id");
    assert!(matches!(
        service.jobs().get(&missing).await,
        Err(Error::NotFound { .. })
    ));
}
