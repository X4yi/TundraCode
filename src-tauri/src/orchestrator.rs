use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tundracode_agents::{DiffProposal, ParsedPlan, TaskStore};
use tundracode_agents::AgentContext;

pub struct AgentOrchestrator {
    pub cancel_token: RwLock<Option<CancellationToken>>,
    pub running: RwLock<bool>,
    pub build_sessions: RwLock<HashMap<String, BuildSession>>,
}

pub struct BuildSession {
    pub run_id: String,
    pub task_store: TaskStore,
    pub parsed_plan: ParsedPlan,
    pub current_proposals: Vec<DiffProposal>,
    pub cancel_token: CancellationToken,
    pub context: AgentContext,
    #[allow(dead_code)]
    pub input: tundracode_agents::AgentInput,
}

impl AgentOrchestrator {
    pub fn new() -> Self {
        Self {
            cancel_token: RwLock::new(None),
            running: RwLock::new(false),
            build_sessions: RwLock::new(HashMap::new()),
        }
    }

    pub async fn is_running(&self) -> bool {
        *self.running.read().await
    }

    pub async fn cancel(&self) {
        if let Some(token) = self.cancel_token.read().await.as_ref() {
            token.cancel();
        }
        *self.running.write().await = false;
    }

    pub async fn store_build_session(&self, session: BuildSession) {
        let run_id = session.run_id.clone();
        self.build_sessions.write().await.insert(run_id, session);
    }

    pub async fn take_build_session(&self, run_id: &str) -> Option<BuildSession> {
        self.build_sessions.write().await.remove(run_id)
    }
}
