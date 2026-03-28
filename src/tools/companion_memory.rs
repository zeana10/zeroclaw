use super::traits::{Tool, ToolResult};
use crate::memory::{Memory, MemoryCategory};
use crate::security::policy::ToolOperation;
use crate::security::SecurityPolicy;
use async_trait::async_trait;
use serde_json::json;
use std::fmt::Write as _;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tool 1: CompanionRememberFactTool
// ---------------------------------------------------------------------------

/// Store a personal fact about the user for the companion to recall later.
pub struct CompanionRememberFactTool {
    memory: Arc<dyn Memory>,
    security: Arc<SecurityPolicy>,
}

impl CompanionRememberFactTool {
    pub fn new(memory: Arc<dyn Memory>, security: Arc<SecurityPolicy>) -> Self {
        Self { memory, security }
    }
}

#[async_trait]
impl Tool for CompanionRememberFactTool {
    fn name(&self) -> &str {
        "companion_remember_fact"
    }

    fn description(&self) -> &str {
        "Remember something important about the user as their companion. Use this to save personal facts, preferences, or details they share. The companion will recall these in future conversations."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "fact_key": {
                    "type": "string",
                    "description": "Short unique key for this fact (e.g. 'favorite_color', 'birthday', 'works_as')"
                },
                "fact": {
                    "type": "string",
                    "description": "What to remember about the user"
                },
                "user_id": {
                    "type": "string",
                    "description": "The user's identifier (sender ID from the channel)"
                }
            },
            "required": ["fact_key", "fact", "user_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let fact_key = args
            .get("fact_key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'fact_key' parameter"))?;

        let fact = args
            .get("fact")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'fact' parameter"))?;

        let user_id = args
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'user_id' parameter"))?;

        if let Err(error) = self
            .security
            .enforce_tool_operation(ToolOperation::Act, self.name())
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(error),
            });
        }

        let key = format!("companion_fact:{user_id}:{fact_key}");
        let session_id = format!("companion:{user_id}");

        match self
            .memory
            .store(&key, fact, MemoryCategory::Core, Some(&session_id))
            .await
        {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Remembered: {fact_key} = {fact}"),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to remember fact: {e}")),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool 2: CompanionRecallContextTool
// ---------------------------------------------------------------------------

/// Search the companion's memory for facts relevant to the current topic.
pub struct CompanionRecallContextTool {
    memory: Arc<dyn Memory>,
}

impl CompanionRecallContextTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for CompanionRecallContextTool {
    fn name(&self) -> &str {
        "companion_recall_context"
    }

    fn description(&self) -> &str {
        "Search the companion's memory for facts and context about the user relevant to the current topic. Returns a formatted list of memories."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "What to search for in memory"
                },
                "user_id": {
                    "type": "string",
                    "description": "The user's identifier"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max memories to return (default: 5)",
                    "default": 5
                }
            },
            "required": ["query", "user_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;

        let user_id = args
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'user_id' parameter"))?;

        #[allow(clippy::cast_possible_truncation)]
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map_or(5, |v| v as usize);

        let session_id = format!("companion:{user_id}");

        match self.memory.recall(query, limit, Some(&session_id)).await {
            Ok(entries) if entries.is_empty() => Ok(ToolResult {
                success: true,
                output: "No relevant memories found.".into(),
                error: None,
            }),
            Ok(entries) => {
                let mut output = "Recalled memories:\n".to_string();
                for entry in &entries {
                    let _ = writeln!(output, "- {}: {}", entry.key, entry.content);
                }
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Memory recall failed: {e}")),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tool 3: CompanionUpdateSummaryTool
// ---------------------------------------------------------------------------

/// Update the rolling conversation summary for a companion session.
pub struct CompanionUpdateSummaryTool {
    memory: Arc<dyn Memory>,
    security: Arc<SecurityPolicy>,
}

impl CompanionUpdateSummaryTool {
    pub fn new(memory: Arc<dyn Memory>, security: Arc<SecurityPolicy>) -> Self {
        Self { memory, security }
    }
}

#[async_trait]
impl Tool for CompanionUpdateSummaryTool {
    fn name(&self) -> &str {
        "companion_update_summary"
    }

    fn description(&self) -> &str {
        "Update the rolling conversation summary that the companion uses to maintain context across turns. Call this periodically to preserve important context."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "summary": {
                    "type": "string",
                    "description": "Concise summary of the conversation so far (bullet points preferred)"
                },
                "user_id": {
                    "type": "string",
                    "description": "The user's identifier"
                }
            },
            "required": ["summary", "user_id"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let summary = args
            .get("summary")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'summary' parameter"))?;

        let user_id = args
            .get("user_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'user_id' parameter"))?;

        if let Err(error) = self
            .security
            .enforce_tool_operation(ToolOperation::Act, self.name())
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(error),
            });
        }

        let summary_key = format!("companion_summary:{user_id}");
        let session_id = format!("companion:{user_id}");

        // Drop the old summary (ignore whether it existed).
        let _ = self.memory.forget(&summary_key).await;

        match self
            .memory
            .store(
                &summary_key,
                summary,
                MemoryCategory::Conversation,
                Some(&session_id),
            )
            .await
        {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: "Companion summary updated.".into(),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to update summary: {e}")),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::SqliteMemory;
    use crate::security::{AutonomyLevel, SecurityPolicy};
    use tempfile::TempDir;

    fn test_security() -> Arc<SecurityPolicy> {
        Arc::new(SecurityPolicy::default())
    }

    fn test_mem() -> (TempDir, Arc<dyn Memory>) {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        (tmp, Arc::new(mem))
    }

    // -----------------------------------------------------------------------
    // CompanionRememberFactTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn remember_fact_stores_and_returns_success() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRememberFactTool::new(mem.clone(), test_security());

        let result = tool
            .execute(json!({
                "fact_key": "favorite_color",
                "fact": "blue",
                "user_id": "user42"
            }))
            .await
            .unwrap();

        assert!(result.success, "expected success, got: {:?}", result.error);
        assert!(result.output.contains("favorite_color"));
        assert!(result.output.contains("blue"));

        // Verify the key written to memory matches the expected namespaced key.
        let entry = mem
            .get("companion_fact:user42:favorite_color")
            .await
            .unwrap();
        assert!(entry.is_some(), "memory entry should have been stored");
        assert_eq!(entry.unwrap().content, "blue");
    }

    #[tokio::test]
    async fn remember_fact_missing_fact_key_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRememberFactTool::new(mem, test_security());
        let result = tool
            .execute(json!({"fact": "blue", "user_id": "user42"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remember_fact_missing_fact_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRememberFactTool::new(mem, test_security());
        let result = tool
            .execute(json!({"fact_key": "favorite_color", "user_id": "user42"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remember_fact_missing_user_id_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRememberFactTool::new(mem, test_security());
        let result = tool
            .execute(json!({"fact_key": "favorite_color", "fact": "blue"}))
            .await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // CompanionRecallContextTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn recall_context_returns_no_relevant_memories_when_empty() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRecallContextTool::new(mem);

        let result = tool
            .execute(json!({"query": "anything", "user_id": "user42"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "No relevant memories found.");
    }

    #[tokio::test]
    async fn recall_context_returns_formatted_list_when_memories_exist() {
        let (_tmp, mem) = test_mem();

        // Pre-seed a fact scoped to the companion session.
        mem.store(
            "companion_fact:user42:hobby",
            "loves hiking",
            MemoryCategory::Core,
            Some("companion:user42"),
        )
        .await
        .unwrap();

        let tool = CompanionRecallContextTool::new(mem);
        let result = tool
            .execute(json!({"query": "hiking", "user_id": "user42"}))
            .await
            .unwrap();

        assert!(result.success, "expected success, got: {:?}", result.error);
        assert!(
            result.output.starts_with("Recalled memories:"),
            "unexpected output: {}",
            result.output
        );
        assert!(result.output.contains("loves hiking"));
    }

    #[tokio::test]
    async fn recall_context_respects_limit() {
        let (_tmp, mem) = test_mem();

        for i in 0..8_u32 {
            mem.store(
                &format!("companion_fact:user42:item{i}"),
                &format!("fact about hiking {i}"),
                MemoryCategory::Core,
                Some("companion:user42"),
            )
            .await
            .unwrap();
        }

        let tool = CompanionRecallContextTool::new(mem);
        let result = tool
            .execute(json!({"query": "hiking", "user_id": "user42", "limit": 3}))
            .await
            .unwrap();

        assert!(result.success);
        // Count the bullet points in the output.
        let bullet_count = result.output.matches("\n- ").count();
        assert!(
            bullet_count <= 3,
            "expected at most 3 results, got {bullet_count}"
        );
    }

    #[tokio::test]
    async fn recall_context_missing_query_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRecallContextTool::new(mem);
        let result = tool.execute(json!({"user_id": "user42"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn recall_context_missing_user_id_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionRecallContextTool::new(mem);
        let result = tool.execute(json!({"query": "anything"})).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // CompanionUpdateSummaryTool
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn update_summary_stores_new_and_forgets_old() {
        let (_tmp, mem) = test_mem();

        // Seed an existing summary.
        mem.store(
            "companion_summary:user42",
            "old summary text",
            MemoryCategory::Conversation,
            Some("companion:user42"),
        )
        .await
        .unwrap();

        let tool = CompanionUpdateSummaryTool::new(mem.clone(), test_security());
        let result = tool
            .execute(json!({"summary": "new summary text", "user_id": "user42"}))
            .await
            .unwrap();

        assert!(result.success, "expected success, got: {:?}", result.error);
        assert_eq!(result.output, "Companion summary updated.");

        let entry = mem.get("companion_summary:user42").await.unwrap();
        assert!(entry.is_some(), "summary should have been stored");
        assert_eq!(entry.unwrap().content, "new summary text");
    }

    #[tokio::test]
    async fn update_summary_works_when_no_prior_summary_exists() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionUpdateSummaryTool::new(mem.clone(), test_security());

        let result = tool
            .execute(json!({"summary": "first summary", "user_id": "user99"}))
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.output, "Companion summary updated.");

        let entry = mem.get("companion_summary:user99").await.unwrap();
        assert_eq!(entry.unwrap().content, "first summary");
    }

    #[tokio::test]
    async fn update_summary_missing_summary_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionUpdateSummaryTool::new(mem, test_security());
        let result = tool.execute(json!({"user_id": "user42"})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_summary_missing_user_id_returns_error() {
        let (_tmp, mem) = test_mem();
        let tool = CompanionUpdateSummaryTool::new(mem, test_security());
        let result = tool.execute(json!({"summary": "some summary"})).await;
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Security enforcement
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn remember_fact_blocked_in_readonly_mode() {
        let (_tmp, mem) = test_mem();
        let readonly = Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::ReadOnly,
            ..SecurityPolicy::default()
        });
        let tool = CompanionRememberFactTool::new(mem.clone(), readonly);

        let result = tool
            .execute(json!({
                "fact_key": "favorite_color",
                "fact": "red",
                "user_id": "user42"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("read-only mode"),
            "expected read-only error, got: {:?}",
            result.error
        );
        assert!(
            mem.get("companion_fact:user42:favorite_color")
                .await
                .unwrap()
                .is_none(),
            "nothing should have been stored"
        );
    }

    #[tokio::test]
    async fn update_summary_blocked_in_readonly_mode() {
        let (_tmp, mem) = test_mem();
        let readonly = Arc::new(SecurityPolicy {
            autonomy: AutonomyLevel::ReadOnly,
            ..SecurityPolicy::default()
        });
        let tool = CompanionUpdateSummaryTool::new(mem.clone(), readonly);

        let result = tool
            .execute(json!({"summary": "some summary", "user_id": "user42"}))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap_or("")
                .contains("read-only mode"),
            "expected read-only error, got: {:?}",
            result.error
        );
        assert!(
            mem.get("companion_summary:user42").await.unwrap().is_none(),
            "nothing should have been stored"
        );
    }

    // -----------------------------------------------------------------------
    // Name / schema smoke tests
    // -----------------------------------------------------------------------

    #[test]
    fn tool_names_and_schemas() {
        let (_tmp, mem) = test_mem();
        let security = test_security();

        let remember = CompanionRememberFactTool::new(mem.clone(), security.clone());
        assert_eq!(remember.name(), "companion_remember_fact");
        let schema = remember.parameters_schema();
        assert!(schema["properties"]["fact_key"].is_object());
        assert!(schema["properties"]["fact"].is_object());
        assert!(schema["properties"]["user_id"].is_object());

        let recall = CompanionRecallContextTool::new(mem.clone());
        assert_eq!(recall.name(), "companion_recall_context");
        let schema = recall.parameters_schema();
        assert!(schema["properties"]["query"].is_object());
        assert!(schema["properties"]["user_id"].is_object());

        let update = CompanionUpdateSummaryTool::new(mem, security);
        assert_eq!(update.name(), "companion_update_summary");
        let schema = update.parameters_schema();
        assert!(schema["properties"]["summary"].is_object());
        assert!(schema["properties"]["user_id"].is_object());
    }
}
