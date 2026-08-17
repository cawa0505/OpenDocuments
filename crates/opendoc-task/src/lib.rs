use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Wire protocol v1 Task Envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskEnvelope {
    pub version: u32,
    pub task_id: String,
    pub task_type: TaskType,
    pub workspace_id: String,
    pub model_ref: String,
    pub payload: serde_json::Value,
}

impl TaskEnvelope {
    pub fn new(
        task_type: TaskType,
        workspace_id: impl Into<String>,
        model_ref: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            version: 1,
            task_id: uuid::Uuid::new_v4().to_string(),
            task_type,
            workspace_id: workspace_id.into(),
            model_ref: model_ref.into(),
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Embed,
    Rerank,
    Infer,
    Parse,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskResult {
    pub task_id: String,
    pub status: TaskStatus,
    pub output: serde_json::Value,
    pub error: Option<String>,
    pub node_id: String,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

impl TaskResult {
    pub fn success(task_id: impl Into<String>, output: serde_json::Value, node_id: impl Into<String>, started_at_ms: u64, finished_at_ms: u64) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Completed,
            output,
            error: None,
            node_id: node_id.into(),
            started_at_ms,
            finished_at_ms,
        }
    }

    pub fn failure(task_id: impl Into<String>, error: impl Into<String>, node_id: impl Into<String>, started_at_ms: u64, finished_at_ms: u64) -> Self {
        Self {
            task_id: task_id.into(),
            status: TaskStatus::Failed,
            output: serde_json::Value::Null,
            error: Some(error.into()),
            node_id: node_id.into(),
            started_at_ms,
            finished_at_ms,
        }
    }
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("Task execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Task cancelled: {0}")]
    Cancelled(String),

    #[error("Unsupported task type: {0:?}")]
    UnsupportedTaskType(TaskType),

    #[error("Worker unavailable: {0}")]
    WorkerUnavailable(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutorMode {
    InProcess,
    SpurDaemon,
    SpurBatch,
}

/// Dispatches tasks either in-process or to a Spur-managed worker.
#[async_trait]
pub trait TaskExecutor: Send + Sync {
    /// Submit one task and await its result (realtime path).
    async fn execute(&self, task: TaskEnvelope) -> Result<TaskResult, TaskError>;

    /// Submit a batch for offline processing (fire-and-forget; Mode 2/3).
    async fn submit_batch(&self, tasks: Vec<TaskEnvelope>) -> Result<Vec<String>, TaskError>;

    fn mode(&self) -> ExecutorMode;
}

/// Default InProcess executor implementation
#[derive(Default)]
pub struct InProcessExecutor;

impl InProcessExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl TaskExecutor for InProcessExecutor {
    async fn execute(&self, task: TaskEnvelope) -> Result<TaskResult, TaskError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(TaskResult::success(
            task.task_id,
            serde_json::json!({ "acknowledged": true, "taskType": task.task_type }),
            "in_process",
            now,
            now,
        ))
    }

    async fn submit_batch(&self, tasks: Vec<TaskEnvelope>) -> Result<Vec<String>, TaskError> {
        let ids = tasks.into_iter().map(|t| t.task_id).collect();
        Ok(ids)
    }

    fn mode(&self) -> ExecutorMode {
        ExecutorMode::InProcess
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_task_envelope_serialization_roundtrip() {
        let task = TaskEnvelope::new(
            TaskType::Embed,
            "workspace-123",
            "bge-m3",
            serde_json::json!({ "texts": ["hello world"] }),
        );

        let serialized = serde_json::to_string(&task).unwrap();
        let deserialized: TaskEnvelope = serde_json::from_str(&serialized).unwrap();

        assert_eq!(task, deserialized);
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.task_type, TaskType::Embed);
        assert_eq!(deserialized.workspace_id, "workspace-123");
    }

    #[tokio::test]
    async fn test_in_process_executor_execution() {
        let executor = InProcessExecutor::new();
        assert_eq!(executor.mode(), ExecutorMode::InProcess);

        let task = TaskEnvelope::new(
            TaskType::Rerank,
            "workspace-123",
            "bge-reranker",
            serde_json::json!({ "query": "test query" }),
        );

        let task_id = task.task_id.clone();
        let result = executor.execute(task).await.unwrap();

        assert_eq!(result.task_id, task_id);
        assert_eq!(result.status, TaskStatus::Completed);
        assert_eq!(result.node_id, "in_process");
    }

    #[tokio::test]
    async fn test_in_process_executor_submit_batch() {
        let executor = InProcessExecutor::new();
        let tasks = vec![
            TaskEnvelope::new(TaskType::Embed, "ws-1", "m", serde_json::json!({})),
            TaskEnvelope::new(TaskType::Infer, "ws-1", "m", serde_json::json!({})),
        ];
        let ids = executor.submit_batch(tasks.clone()).await.unwrap();
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0], tasks[0].task_id);
        assert_eq!(ids[1], tasks[1].task_id);
    }
}
