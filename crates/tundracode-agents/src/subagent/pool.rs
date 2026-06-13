use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Semaphore;

use crate::events::{SubagentEvent, SubagentEventBus};
use crate::subagent::types::{SubagentRequest, SubagentResult};

pub struct SubagentPool {
    max_concurrent: usize,
    budget_total: u32,
    budget_used: Arc<AtomicU32>,
    semaphore: Arc<Semaphore>,
    event_bus: Arc<SubagentEventBus>,
}

impl SubagentPool {
    pub fn new(max_concurrent: usize, budget_total: u32, event_bus: Arc<SubagentEventBus>) -> Self {
        Self {
            max_concurrent,
            budget_total,
            budget_used: Arc::new(AtomicU32::new(0)),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            event_bus,
        }
    }

    pub fn budget_remaining(&self) -> u32 {
        self.budget_total.saturating_sub(self.budget_used.load(Ordering::Relaxed))
    }

    pub fn budget_used(&self) -> u32 {
        self.budget_used.load(Ordering::Relaxed)
    }

    pub fn utilization(&self) -> f32 {
        if self.budget_total == 0 {
            return 0.0;
        }
        self.budget_used.load(Ordering::Relaxed) as f32 / self.budget_total as f32
    }

    pub fn remaining_slots(&self) -> usize {
        self.semaphore.available_permits()
    }

    pub fn can_spawn(&self, _profile: &str) -> bool {
        self.budget_remaining() > 0 && self.semaphore.available_permits() > 0
    }

    pub async fn execute_batch(
        &self,
        requests: Vec<SubagentRequest>,
        executor: impl Fn(SubagentRequest) -> futures::future::BoxFuture<'static, SubagentResult> + Send + Sync + 'static,
    ) -> Vec<SubagentResult> {
        use futures::future::join_all;

        let executor = Arc::new(executor);
        let mut handles = Vec::new();

        for request in requests {
            let permit = self.semaphore.clone().acquire_owned().await;
            if permit.is_err() {
                continue;
            }

            let executor = executor.clone();
            let budget_used = self.budget_used.clone();
            let event_bus = self.event_bus.clone();

            let handle = tokio::spawn(async move {
                let subagent_id = format!("sub_{}", now_millis());

                event_bus.emit(SubagentEvent::created(
                    &subagent_id,
                    &request.agent_profile_id,
                    &request.task_description,
                ));

                let start = std::time::Instant::now();
                let result = executor(request).await;

                budget_used.fetch_add(result.tokens_used, Ordering::Relaxed);

                if result.success {
                    event_bus.emit(SubagentEvent::completed(
                        &subagent_id,
                        &result.summary,
                        result.key_findings.clone(),
                        result.files_referenced.clone(),
                        result.tokens_used,
                        start.elapsed().as_millis() as u64,
                    ));
                } else {
                    event_bus.emit(SubagentEvent::failed(
                        &subagent_id,
                        result.error.as_deref().unwrap_or("unknown error"),
                        start.elapsed().as_millis() as u64,
                    ));
                }

                drop(permit);
                result
            });

            handles.push(handle);
        }

        let results = join_all(handles)
            .await
            .into_iter()
            .filter_map(|h| h.ok())
            .collect::<Vec<_>>();

        results
    }

    pub async fn execute_single(
        &self,
        request: SubagentRequest,
        executor: impl Fn(SubagentRequest) -> futures::future::BoxFuture<'static, SubagentResult> + Send + Sync + 'static,
    ) -> SubagentResult {
        let permit = self.semaphore.clone().acquire_owned().await;
        if permit.is_err() {
            return SubagentResult {
                agent_id: request.agent_profile_id.clone(),
                summary: "No available slots in subagent pool".to_string(),
                full_output: None,
                tokens_used: 0,
                success: false,
                error: Some("Pool exhausted".to_string()),
                key_findings: vec![],
                files_referenced: vec![],
            };
        }

        let subagent_id = format!("sub_{}", now_millis());
        let executor = Arc::new(executor);
        let budget_used = self.budget_used.clone();
        let event_bus = self.event_bus.clone();

        event_bus.emit(SubagentEvent::created(
            &subagent_id,
            &request.agent_profile_id,
            &request.task_description,
        ));

        let start = std::time::Instant::now();
        let executor_clone = executor.clone();
        let result = executor_clone(request).await;

        budget_used.fetch_add(result.tokens_used, Ordering::Relaxed);

        if result.success {
            event_bus.emit(SubagentEvent::completed(
                &subagent_id,
                &result.summary,
                result.key_findings.clone(),
                result.files_referenced.clone(),
                result.tokens_used,
                start.elapsed().as_millis() as u64,
            ));
        } else {
            event_bus.emit(SubagentEvent::failed(
                &subagent_id,
                result.error.as_deref().unwrap_or("unknown error"),
                start.elapsed().as_millis() as u64,
            ));
        }

        drop(permit);
        result
    }

    pub fn event_bus(&self) -> &SubagentEventBus {
        &self.event_bus
    }
}

fn now_millis() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
