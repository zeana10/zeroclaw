//! Post-turn passive memory harvester for ClawdCompanion.
//!
//! After every conversation turn, [`CompanionMemoryHarvester::harvest`] makes a
//! lightweight LLM call to extract personal facts the user shared and stores them
//! in the memory backend — without the user or companion needing to explicitly
//! invoke a memory tool. This is the mechanism behind Tolan-style persistent
//! memory across sessions.
//!
//! Facts are stored under keys of the form `companion_fact:{user_id}:{slug}` and
//! scoped to the `companion:{user_id}` session so the context builder recalls them
//! on future turns.

use crate::memory::{Memory, MemoryCategory};
use crate::providers::Provider;
use std::sync::Arc;

// Helper trait alias so the signature is clear.
type ArcMemory = Arc<dyn Memory>;

/// Harvests personal facts from a conversation exchange and persists them.
pub struct CompanionMemoryHarvester {
    /// Maximum facts to extract per turn.
    pub max_facts_per_turn: usize,
}

impl CompanionMemoryHarvester {
    pub fn new(max_facts_per_turn: usize) -> Self {
        Self { max_facts_per_turn }
    }

    /// Extract and store personal facts from one conversation turn.
    ///
    /// Sends a compact LLM prompt asking for a JSON array of `{key, fact}` pairs
    /// drawn from `user_message` and `assistant_response`, then stores each fact
    /// under `companion_fact:{user_id}:{key}` scoped to the companion session.
    ///
    /// Returns the number of facts saved. Errors are logged and swallowed so a
    /// harvesting failure never interrupts the user-facing response.
    pub async fn harvest(
        &self,
        user_id: &str,
        user_message: &str,
        assistant_response: &str,
        memory: &ArcMemory,
        provider: &dyn Provider,
        model: &str,
    ) -> usize {
        let facts = match self
            .extract_facts(user_message, assistant_response, provider, model)
            .await
        {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!("companion harvester: fact extraction failed: {e}");
                return 0;
            }
        };

        let session_scope = format!("companion:{user_id}");
        let mut saved = 0usize;

        for (key, fact) in facts.into_iter().take(self.max_facts_per_turn) {
            if key.is_empty() || fact.is_empty() {
                continue;
            }
            let mem_key = format!("companion_fact:{user_id}:{key}");
            match memory
                .store(&mem_key, &fact, MemoryCategory::Core, Some(&session_scope))
                .await
            {
                Ok(()) => {
                    saved += 1;
                    tracing::debug!("companion harvester: saved fact '{key}' for {user_id}");
                }
                Err(e) => {
                    tracing::debug!("companion harvester: failed to save fact '{key}': {e}");
                }
            }
        }

        saved
    }

    /// Call the LLM with a minimal extraction prompt and parse the JSON response.
    async fn extract_facts(
        &self,
        user_message: &str,
        assistant_response: &str,
        provider: &dyn Provider,
        model: &str,
    ) -> anyhow::Result<Vec<(String, String)>> {
        let system = "You are a personal fact extractor. \
            Given one exchange from a conversation, identify any personal facts \
            the USER shared about themselves (name, job, location, preferences, \
            relationships, hobbies, goals, health, etc.). \
            Return ONLY a compact JSON array: \
            [{\"key\":\"snake_case_label\",\"fact\":\"one sentence\"}] \
            or [] if nothing personal was shared. No explanation, no markdown fences.";

        let prompt = format!(
            "User: {user_message}\nAssistant: {assistant_response}\n\nExtract personal facts:"
        );

        let raw = provider
            .chat_with_system(Some(system), &prompt, model, 0.0)
            .await?;

        parse_facts_json(&raw)
    }
}

/// Parse a JSON array of `{key, fact}` objects from an LLM response string.
/// Tolerant: strips leading/trailing whitespace and markdown fences before parsing.
fn parse_facts_json(raw: &str) -> anyhow::Result<Vec<(String, String)>> {
    // Strip common LLM wrapping (```json ... ``` or ``` ... ```)
    let trimmed = raw.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_end_matches("```").trim())
        .unwrap_or(trimmed);

    // Find JSON array bounds
    let start = stripped.find('[').ok_or_else(|| anyhow::anyhow!("no JSON array found"))?;
    let end = stripped.rfind(']').ok_or_else(|| anyhow::anyhow!("unclosed JSON array"))?;
    let json_slice = &stripped[start..=end];

    let arr: serde_json::Value = serde_json::from_str(json_slice)?;
    let items = arr.as_array().ok_or_else(|| anyhow::anyhow!("expected JSON array"))?;

    let mut results = Vec::new();
    for item in items {
        let key = item
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        let fact = item
            .get("fact")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if !key.is_empty() && !fact.is_empty() {
            results.push((key, fact));
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_clean_json() {
        let raw = r#"[{"key":"occupation","fact":"Works as a software engineer"},{"key":"city","fact":"Lives in Berlin"}]"#;
        let facts = parse_facts_json(raw).unwrap();
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].0, "occupation");
        assert_eq!(facts[0].1, "Works as a software engineer");
        assert_eq!(facts[1].0, "city");
    }

    #[test]
    fn parse_empty_array() {
        let raw = "[]";
        let facts = parse_facts_json(raw).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn parse_markdown_fenced() {
        let raw = "```json\n[{\"key\":\"name\",\"fact\":\"User's name is Alex\"}]\n```";
        let facts = parse_facts_json(raw).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].0, "name");
    }

    #[test]
    fn parse_strips_explanation_before_array() {
        let raw = "Sure! Here are the facts:\n[{\"key\":\"pet\",\"fact\":\"Has a dog named Max\"}]";
        let facts = parse_facts_json(raw).unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].0, "pet");
    }

    #[test]
    fn parse_skips_entries_with_empty_fields() {
        let raw = r#"[{"key":"","fact":"something"},{"key":"job","fact":""}]"#;
        let facts = parse_facts_json(raw).unwrap();
        assert!(facts.is_empty());
    }

    #[test]
    fn parse_no_array_returns_error() {
        let raw = "Nothing to see here.";
        assert!(parse_facts_json(raw).is_err());
    }
}
