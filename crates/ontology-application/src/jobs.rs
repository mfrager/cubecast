//! Background job execution for classification, embedding, and inference
//! batches.
//!
//! Jobs run on the current Tokio runtime with bounded concurrency, report
//! progress, record errors, can be cancelled, and can be retried from their
//! stored request.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use chrono::Utc;
use ontology_classify::ClassificationService;
use ontology_core::error::{Error, Result};
use ontology_core::external::ExternalOntologyRegistry;
use ontology_core::id::{Id, OperationId};
use ontology_core::job::{JobRequest, JobState, JobStatus};
use ontology_core::rdf::RdfStore;
use ontology_core::semantic::InferenceEngine;
use ontology_document::DocumentService;
use ontology_embedding::EmbeddingService;
use ontology_import::ExternalOntologyManager;
use ontology_inference::RuleInferenceEngine;
use tokio::runtime::Handle;
use tokio::sync::Semaphore;

/// Shared engines a job may use.
pub struct JobContext {
    /// Classification engine.
    pub classify: Arc<ClassificationService>,
    /// Embedding service.
    pub embedding: Arc<EmbeddingService>,
    /// Inference engine.
    pub inference: Arc<RuleInferenceEngine>,
    /// RDF store used by import and export jobs.
    pub store: Arc<dyn RdfStore>,
    /// External ontology registry used by import and sync jobs.
    pub external: Arc<ExternalOntologyManager>,
    /// Document service used by extraction jobs.
    pub documents: Arc<RwLock<Option<Arc<DocumentService>>>>,
}

struct JobRecord {
    status: JobStatus,
    request: JobRequest,
    cancel: Arc<AtomicBool>,
}

enum JobFailure {
    Cancelled,
    Failed(Error),
}

/// In-process job registry and worker pool.
pub struct JobManager {
    records: Arc<RwLock<BTreeMap<Id, JobRecord>>>,
    semaphore: Arc<Semaphore>,
    context: Arc<JobContext>,
}

impl JobManager {
    /// Creates a manager with the given worker concurrency.
    #[must_use]
    pub fn new(context: Arc<JobContext>, concurrency: usize) -> Self {
        Self {
            records: Arc::new(RwLock::new(BTreeMap::new())),
            semaphore: Arc::new(Semaphore::new(concurrency.max(1))),
            context,
        }
    }

    /// Submits a job and returns its initial status.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Unsupported`] when called outside a Tokio runtime.
    pub async fn submit(&self, request: JobRequest) -> Result<JobStatus> {
        let handle = Handle::try_current().map_err(|_| Error::Unsupported {
            operation: "background jobs outside a Tokio runtime".to_owned(),
        })?;
        self.enqueue(&handle, request, 0)
    }

    /// Retrieves a job status.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] for an unknown job.
    pub async fn get(&self, id: &Id) -> Result<JobStatus> {
        let records = self.read()?;
        records
            .get(id)
            .map(|record| record.status.clone())
            .ok_or_else(|| Error::NotFound {
                kind: "job",
                id: id.to_string(),
            })
    }

    /// Lists jobs, oldest first.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry lock is poisoned.
    pub async fn list(&self) -> Result<Vec<JobStatus>> {
        let records = self.read()?;
        let mut statuses: Vec<JobStatus> = records
            .values()
            .map(|record| record.status.clone())
            .collect();
        statuses.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(statuses)
    }

    /// Requests cancellation. Jobs already finished cannot be cancelled.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] or [`Error::Conflict`].
    pub async fn cancel(&self, id: &Id) -> Result<JobStatus> {
        let mut records = self.write()?;
        let record = records.get_mut(id).ok_or_else(|| Error::NotFound {
            kind: "job",
            id: id.to_string(),
        })?;
        if record.status.state.is_terminal() {
            return Err(Error::Conflict {
                kind: "job",
                id: id.to_string(),
            });
        }
        record.cancel.store(true, Ordering::SeqCst);
        record.status.state = JobState::Cancelled;
        record.status.message = Some("cancellation requested".to_owned());
        record.status.finished_at = Some(Utc::now());
        Ok(record.status.clone())
    }

    /// Re-enqueues a failed or cancelled job.
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotFound`] when the job is unknown and
    /// [`Error::Conflict`] when it is not failed or cancelled.
    pub async fn retry(&self, id: &Id) -> Result<JobStatus> {
        let handle = Handle::try_current().map_err(|_| Error::Unsupported {
            operation: "background jobs outside a Tokio runtime".to_owned(),
        })?;
        let (request, status, cancel) = {
            let mut records = self.write()?;
            let record = records.get_mut(id).ok_or_else(|| Error::NotFound {
                kind: "job",
                id: id.to_string(),
            })?;
            if !matches!(record.status.state, JobState::Failed | JobState::Cancelled) {
                return Err(Error::Conflict {
                    kind: "job",
                    id: id.to_string(),
                });
            }
            let cancel = Arc::new(AtomicBool::new(false));
            record.cancel = Arc::clone(&cancel);
            record.status.state = JobState::Queued;
            record.status.progress = 0.0;
            record.status.processed = 0;
            record.status.attempts += 1;
            record.status.error = None;
            record.status.result = None;
            record.status.started_at = None;
            record.status.finished_at = None;
            record.status.message = Some("queued".to_owned());
            (record.request.clone(), record.status.clone(), cancel)
        };
        self.spawn(handle, id.clone(), request, status.attempts, cancel);
        Ok(status)
    }

    fn enqueue(&self, handle: &Handle, request: JobRequest, attempts: u32) -> Result<JobStatus> {
        let id = Id::new(format!("urn:onto:job:{}", OperationId::generate()))?;
        let total = total_items(&request);
        let status = JobStatus {
            id: id.clone(),
            kind: request.kind().to_owned(),
            state: JobState::Queued,
            progress: 0.0,
            processed: 0,
            total,
            attempts,
            message: Some("queued".to_owned()),
            error: None,
            result: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut records = self.write()?;
            records.insert(
                id.clone(),
                JobRecord {
                    status: status.clone(),
                    request: request.clone(),
                    cancel: Arc::clone(&cancel),
                },
            );
        }
        self.spawn(handle.clone(), id, request, attempts, cancel);
        Ok(status)
    }

    fn spawn(
        &self,
        handle: Handle,
        id: Id,
        request: JobRequest,
        attempts: u32,
        cancel: Arc<AtomicBool>,
    ) {
        let records = Arc::clone(&self.records);
        let semaphore = Arc::clone(&self.semaphore);
        let context = Arc::clone(&self.context);
        handle.spawn(async move {
            let permit = semaphore.acquire_owned().await;
            if cancel.load(Ordering::SeqCst) {
                mark(&records, &id, |status| {
                    status.state = JobState::Cancelled;
                    status.finished_at = Some(Utc::now());
                    status.message = Some("cancelled before start".to_owned());
                });
                return;
            }
            {
                let mut guard = match records.write() {
                    Ok(guard) => guard,
                    Err(_) => return,
                };
                if let Some(record) = guard.get_mut(&id) {
                    record.status.state = JobState::Running;
                    record.status.started_at = Some(Utc::now());
                    record.status.message = Some("running".to_owned());
                }
            }
            let outcome = run(&context, &request, &cancel, &records, &id).await;
            let _ = permit;
            match outcome {
                Ok(value) => mark(&records, &id, |status| {
                    status.state = JobState::Completed;
                    status.progress = 1.0;
                    status.processed = status.total;
                    status.message = Some("completed".to_owned());
                    status.result = Some(value);
                    status.finished_at = Some(Utc::now());
                }),
                Err(JobFailure::Cancelled) => mark(&records, &id, |status| {
                    status.state = JobState::Cancelled;
                    status.message = Some("cancelled".to_owned());
                    status.finished_at = Some(Utc::now());
                }),
                Err(JobFailure::Failed(error)) => {
                    let message = error.to_string();
                    mark(&records, &id, |status| {
                        status.state = JobState::Failed;
                        status.error = Some(message);
                        status.finished_at = Some(Utc::now());
                    });
                }
            }
            let _ = attempts;
        });
    }

    fn read(&self) -> Result<std::sync::RwLockReadGuard<'_, BTreeMap<Id, JobRecord>>> {
        self.records
            .read()
            .map_err(|_| Error::validation("job registry lock is poisoned"))
    }

    fn write(&self) -> Result<std::sync::RwLockWriteGuard<'_, BTreeMap<Id, JobRecord>>> {
        self.records
            .write()
            .map_err(|_| Error::validation("job registry lock is poisoned"))
    }
}

fn total_items(request: &JobRequest) -> usize {
    match request {
        JobRequest::ClassifyBatch { instances, .. }
        | JobRequest::EmbedBatch { instances, .. }
        | JobRequest::InferBatch { instances, .. }
        | JobRequest::ReclassifyBatch { instances, .. } => instances.len(),
        JobRequest::DocumentExtract(_) => 1,
        JobRequest::RdfImport { .. }
        | JobRequest::RdfExport { .. }
        | JobRequest::ExternalImport(_)
        | JobRequest::ExternalSync { .. } => 1,
    }
}

fn mark(records: &RwLock<BTreeMap<Id, JobRecord>>, id: &Id, update: impl FnOnce(&mut JobStatus)) {
    if let Ok(mut guard) = records.write() {
        if let Some(record) = guard.get_mut(id) {
            update(&mut record.status);
        }
    }
}

async fn run(
    context: &JobContext,
    request: &JobRequest,
    cancel: &AtomicBool,
    records: &RwLock<BTreeMap<Id, JobRecord>>,
    id: &Id,
) -> std::result::Result<serde_json::Value, JobFailure> {
    let total = total_items(request);
    match request {
        JobRequest::ClassifyBatch { instances, options } => {
            let mut results = Vec::new();
            for (index, instance) in instances.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    return Err(JobFailure::Cancelled);
                }
                let found = context
                    .classify
                    .classify(instance, options.clone())
                    .await
                    .map_err(JobFailure::Failed)?;
                results.extend(found);
                report_progress(records, id, index, total);
            }
            serde_json::to_value(results)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::EmbedBatch {
            instances,
            attributes,
        } => {
            let mut results = Vec::new();
            for (index, instance) in instances.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    return Err(JobFailure::Cancelled);
                }
                let found = context
                    .embedding
                    .generate(instance, attributes, None)
                    .await
                    .map_err(JobFailure::Failed)?;
                results.extend(found);
                report_progress(records, id, index, total);
            }
            serde_json::to_value(results)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::InferBatch {
            instances,
            options,
            policy,
        } => {
            let mut results = Vec::new();
            for (index, instance) in instances.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    return Err(JobFailure::Cancelled);
                }
                let report = context
                    .inference
                    .apply(instance, *policy, options.clone())
                    .await
                    .map_err(JobFailure::Failed)?;
                results.push(report);
                report_progress(records, id, index, total);
            }
            serde_json::to_value(results)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::ReclassifyBatch { instances, options } => {
            let mut results = Vec::new();
            for (index, instance) in instances.iter().enumerate() {
                if cancel.load(Ordering::SeqCst) {
                    return Err(JobFailure::Cancelled);
                }
                let found = context
                    .classify
                    .reclassify(instance, options.clone())
                    .await
                    .map_err(JobFailure::Failed)?;
                results.extend(found);
                report_progress(records, id, index, total);
            }
            serde_json::to_value(results)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::RdfImport {
            format,
            data,
            graph,
        } => {
            let format = format
                .parse::<ontology_core::format::RdfFormat>()
                .map_err(JobFailure::Failed)?;
            let inserted = context
                .store
                .load_from_slice(format, data.as_bytes(), None, graph.as_ref())
                .map_err(JobFailure::Failed)?;
            Ok(serde_json::json!({ "imported": inserted }))
        }
        JobRequest::RdfExport { format, graph } => {
            let format = format
                .parse::<ontology_core::format::RdfFormat>()
                .map_err(JobFailure::Failed)?;
            let data = context
                .store
                .serialize(format, graph.as_ref())
                .map_err(JobFailure::Failed)?;
            Ok(serde_json::json!({
                "format": format.as_str(),
                "data": data
            }))
        }
        JobRequest::ExternalImport(request) => {
            let report = context
                .external
                .import(request.clone())
                .await
                .map_err(JobFailure::Failed)?;
            serde_json::to_value(report)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::ExternalSync { id: external } => {
            let report = context
                .external
                .sync(external)
                .await
                .map_err(JobFailure::Failed)?;
            serde_json::to_value(report)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
        JobRequest::DocumentExtract(request) => {
            let documents = context
                .documents
                .read()
                .ok()
                .and_then(|slot| slot.clone())
                .ok_or_else(|| {
                    JobFailure::Failed(Error::Unsupported {
                        operation: "document extraction is not configured".to_owned(),
                    })
                })?;
            let output = documents
                .extract(request.clone())
                .await
                .map_err(JobFailure::Failed)?;
            serde_json::to_value(output)
                .map_err(Error::serialization)
                .map_err(JobFailure::Failed)
        }
    }
}

fn report_progress(records: &RwLock<BTreeMap<Id, JobRecord>>, id: &Id, index: usize, total: usize) {
    mark(records, id, |status| {
        status.processed = index + 1;
        status.progress = if total == 0 {
            1.0
        } else {
            ((index + 1) as f64 / total as f64).min(1.0)
        };
    });
}
