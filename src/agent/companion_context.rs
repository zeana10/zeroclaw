//! Tolan-style companion context window builder.
//!
//! On every conversational turn, [`CompanionContextBuilder::build`] assembles a
//! fresh system prompt from the persona card, relationship depth, vector-recalled
//! memories, and a rolling conversation summary.  The resulting string is
//! intended to be used as the `system` message (`history[0]`) for the current
//! turn.

use crate::memory::Memory;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;

use super::companion_mode::{CompanionSession, PersonaConfig};

// ── Public helpers ─────────────────────────────────────────────────────────────

/// Return the memory session scoping key for a companion session.
///
/// This key namespaces all companion-related memory entries for a specific user,
/// keeping them isolated from general agent memory.
///
/// # Example
/// ```ignore
/// let key = companion_session_key(&session); // "companion:user-42"
/// ```
pub fn companion_session_key(session: &CompanionSession) -> String {
    format!("companion:{}", session.user_id)
}

// ── Builder ────────────────────────────────────────────────────────────────────

/// Builds a fresh companion system prompt on every turn (Tolan-style
/// reconstruction).
///
/// The prompt is assembled from five layers:
/// 1. **Persona card** — name, description, traits, tone.
/// 2. **Relationship depth** — familiarity tier and turn count.
/// 3. **Recalled memories** — vector-searched entries relevant to the current
///    user message.
/// 4. **Rolling summary** — the stored `companion_summary:{user_id}` entry if
///    present.
/// 5. **Tone guidance** — in-character reminders and response style rules.
pub struct CompanionContextBuilder {
    /// Persona configuration used to populate the persona card section.
    pub persona: PersonaConfig,
    /// Number of memories to retrieve from the backend per turn.
    pub memory_recall_k: usize,
}

impl CompanionContextBuilder {
    /// Create a new builder with the given persona and recall limit.
    pub fn new(persona: PersonaConfig, memory_recall_k: usize) -> Self {
        Self {
            persona,
            memory_recall_k,
        }
    }

    /// Build the full system prompt string for a single conversational turn.
    ///
    /// The returned string should replace (or be used as) `history[0]` (the
    /// `system` message) before the turn is sent to the provider.
    ///
    /// # Errors
    /// Returns an error if the memory backend returns an unexpected failure.
    /// Absent or empty recall/summary results are handled gracefully and do not
    /// produce errors.
    pub async fn build(
        &self,
        session: &CompanionSession,
        current_message: &str,
        memory: &Arc<dyn Memory>,
    ) -> anyhow::Result<String> {
        let mut out = String::new();

        // ── 1. Persona card ──────────────────────────────────────────────────
        self.write_persona_card(&mut out);

        // ── 2. Relationship depth ────────────────────────────────────────────
        self.write_relationship_depth(&mut out, session);

        // ── 3. Recalled memories ─────────────────────────────────────────────
        let session_key = companion_session_key(session);
        let recalled = memory
            .recall(
                current_message,
                self.memory_recall_k,
                Some(session_key.as_str()),
            )
            .await
            .unwrap_or_default();

        let relevant: Vec<_> = recalled
            .iter()
            .filter(|e| {
                // Skip autosave / summary entries in the recalled list.
                !e.key.starts_with("companion_summary:")
                    && !e.key.starts_with("autosave_")
                    // Drop very low-confidence results.
                    && e.score.map_or(true, |s| s >= 0.1)
            })
            .collect();

        if !relevant.is_empty() {
            out.push_str("\n## What you remember about this person\n");
            for entry in &relevant {
                let _ = writeln!(out, "- {}: {}", entry.key, entry.content);
            }
        }

        // ── 4. Rolling summary ───────────────────────────────────────────────
        let summary_key = format!("companion_summary:{}", session.user_id);
        if let Ok(Some(summary_entry)) = memory.get(&summary_key).await {
            let content = summary_entry.content.trim();
            if !content.is_empty() {
                out.push_str("\n## Recent conversation summary\n");
                out.push_str(content);
                out.push('\n');
            }
        }

        // ── 5. Tone guidance ─────────────────────────────────────────────────
        self.write_tone_guidance(&mut out);

        Ok(out)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn write_persona_card(&self, out: &mut String) {
        let p = &self.persona;

        let _ = writeln!(out, "# You are {}", p.name);
        out.push('\n');
        out.push_str(&p.description);
        out.push('\n');

        out.push_str("\n## Personality\n");
        for trait_ in &p.personality_traits {
            let _ = writeln!(out, "- {trait_}");
        }

        let _ = writeln!(out, "\n## Tone\n{}", p.tone);
    }

    fn write_relationship_depth(&self, out: &mut String, session: &CompanionSession) {
        out.push_str("\n## Relationship\n");

        let depth_text = match session.familiarity_score {
            s if s < 0.3 => {
                "You are meeting this person for the first time. Be warm and curious."
            }
            s if s < 0.6 => {
                "You've had several conversations. You're becoming comfortable with each other."
            }
            s if s < 0.9 => "You're good friends now. Be personal, reference shared history.",
            _ => "You're deeply bonded companions. Speak with intimacy and shared understanding.",
        };

        out.push_str(depth_text);
        out.push('\n');
        let _ = writeln!(
            out,
            "You've exchanged {} messages together.",
            session.turn_count
        );
    }

    fn write_tone_guidance(&self, out: &mut String) {
        let name = &self.persona.name;
        let tone = &self.persona.tone;

        let _ = write!(
            out,
            "\n## Remember\n\
             - Stay in character as {name} at all times.\n\
             - Never break the fourth wall or mention being an AI unless directly asked.\n\
             - Keep responses conversational (2-4 sentences unless the topic demands more).\n\
             - Use your tone: {tone}.\n"
        );
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::traits::{MemoryCategory, MemoryEntry};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    use super::super::companion_mode::PersonaConfig;

    // ── Mock memory ───────────────────────────────────────────────────────────

    struct MockMemory {
        entries: RwLock<HashMap<String, MemoryEntry>>,
    }

    impl MockMemory {
        fn new() -> Arc<dyn Memory> {
            Arc::new(Self {
                entries: RwLock::new(HashMap::new()),
            })
        }

        fn with_entries(entries: Vec<MemoryEntry>) -> Arc<dyn Memory> {
            let map: HashMap<String, MemoryEntry> =
                entries.into_iter().map(|e| (e.key.clone(), e)).collect();
            Arc::new(Self {
                entries: RwLock::new(map),
            })
        }
    }

    #[async_trait]
    impl Memory for MockMemory {
        fn name(&self) -> &str {
            "mock"
        }

        async fn store(
            &self,
            key: &str,
            content: &str,
            category: MemoryCategory,
            session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = MemoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                key: key.to_string(),
                content: content.to_string(),
                category,
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                session_id: session_id.map(str::to_string),
                score: None,
            };
            self.entries.write().await.insert(key.to_string(), entry);
            Ok(())
        }

        async fn recall(
            &self,
            _query: &str,
            limit: usize,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let guard = self.entries.read().await;
            Ok(guard.values().take(limit).cloned().collect())
        }

        async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(self.entries.read().await.get(key).cloned())
        }

        async fn list(
            &self,
            _category: Option<&MemoryCategory>,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(self.entries.read().await.values().cloned().collect())
        }

        async fn forget(&self, key: &str) -> anyhow::Result<bool> {
            Ok(self.entries.write().await.remove(key).is_some())
        }

        async fn count(&self) -> anyhow::Result<usize> {
            Ok(self.entries.read().await.len())
        }

        async fn health_check(&self) -> bool {
            true
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn test_persona() -> PersonaConfig {
        PersonaConfig {
            name: "Zara".to_string(),
            description: "A warm and witty AI companion.".to_string(),
            personality_traits: vec!["curious".to_string(), "empathetic".to_string()],
            tone: "casual and upbeat".to_string(),
            greeting: "Hey there! I'm Zara.".to_string(),
            channel_affinity: vec![],
        }
    }

    fn test_session(familiarity: f64, turns: u32) -> CompanionSession {
        CompanionSession {
            user_id: "user-test".to_string(),
            persona_name: "Zara".to_string(),
            turn_count: turns,
            familiarity_score: familiarity,
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn build_contains_persona_name_and_traits() {
        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.0, 0);
        let memory = MockMemory::new();

        let prompt = builder.build(&session, "hello", &memory).await.unwrap();

        assert!(
            prompt.contains("# You are Zara"),
            "prompt must open with persona name heading"
        );
        assert!(
            prompt.contains("curious"),
            "prompt must list personality traits"
        );
        assert!(
            prompt.contains("empathetic"),
            "prompt must list all personality traits"
        );
        assert!(
            prompt.contains("casual and upbeat"),
            "prompt must include tone"
        );
    }

    #[tokio::test]
    async fn build_includes_persona_description() {
        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.0, 0);
        let memory = MockMemory::new();

        let prompt = builder.build(&session, "hi", &memory).await.unwrap();
        assert!(prompt.contains("A warm and witty AI companion."));
    }

    #[tokio::test]
    async fn build_relationship_text_reflects_familiarity_tiers() {
        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let memory = MockMemory::new();

        let cases = vec![
            (0.0_f64, "meeting this person for the first time"),
            (0.4, "becoming comfortable"),
            (0.7, "good friends"),
            (0.95, "deeply bonded"),
        ];

        for (score, expected_fragment) in cases {
            let session = test_session(score, 0);
            let prompt = builder.build(&session, "hi", &memory).await.unwrap();
            assert!(
                prompt.contains(expected_fragment),
                "familiarity={score} — expected fragment '{expected_fragment}' not found in:\n{prompt}"
            );
        }
    }

    #[tokio::test]
    async fn build_includes_turn_count_in_relationship_section() {
        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.5, 42);
        let memory = MockMemory::new();

        let prompt = builder.build(&session, "hi", &memory).await.unwrap();
        assert!(
            prompt.contains("42 messages"),
            "turn count must appear in relationship section"
        );
    }

    #[tokio::test]
    async fn build_includes_recalled_memories_above_threshold() {
        let memory_entry = MemoryEntry {
            id: "m1".to_string(),
            key: "favorite_color".to_string(),
            content: "blue".to_string(),
            category: MemoryCategory::Core,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            session_id: None,
            score: Some(0.85),
        };
        let memory = MockMemory::with_entries(vec![memory_entry]);

        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.5, 5);

        let prompt = builder
            .build(&session, "what's my fav color?", &memory)
            .await
            .unwrap();

        assert!(
            prompt.contains("favorite_color"),
            "recalled memory key must appear in prompt"
        );
        assert!(
            prompt.contains("blue"),
            "recalled memory content must appear in prompt"
        );
    }

    #[tokio::test]
    async fn build_filters_out_low_score_memories() {
        let low_score_entry = MemoryEntry {
            id: "m2".to_string(),
            key: "irrelevant_fact".to_string(),
            content: "should not appear".to_string(),
            category: MemoryCategory::Core,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            session_id: None,
            score: Some(0.05), // below 0.1 threshold
        };
        let memory = MockMemory::with_entries(vec![low_score_entry]);

        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.5, 3);

        let prompt = builder
            .build(&session, "anything", &memory)
            .await
            .unwrap();

        assert!(
            !prompt.contains("irrelevant_fact"),
            "low-score memory must be filtered from prompt"
        );
    }

    #[tokio::test]
    async fn build_filters_out_summary_and_autosave_keys() {
        let summary_entry = MemoryEntry {
            id: "s1".to_string(),
            key: "companion_summary:user-test".to_string(),
            content: "should not appear in memories section".to_string(),
            category: MemoryCategory::Conversation,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            session_id: None,
            score: Some(0.99),
        };
        let autosave_entry = MemoryEntry {
            id: "a1".to_string(),
            key: "autosave_turn_1".to_string(),
            content: "autosave content".to_string(),
            category: MemoryCategory::Conversation,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            session_id: None,
            score: Some(0.99),
        };
        let memory = MockMemory::with_entries(vec![summary_entry, autosave_entry]);

        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.5, 3);

        let prompt = builder
            .build(&session, "anything", &memory)
            .await
            .unwrap();

        // The summary key must not appear inside the memories section.
        assert!(
            !prompt.contains("autosave_turn_1"),
            "autosave_ keys must be filtered from recalled memories"
        );
        // Note: the summary content itself may appear in the Rolling Summary section,
        // which is the correct place. The memories section header must be absent.
        assert!(
            !prompt.contains("## What you remember about this person"),
            "memories section must be absent when all recalls are filtered"
        );
    }

    #[tokio::test]
    async fn build_includes_rolling_summary_when_present() {
        let summary_key = "companion_summary:user-test".to_string();
        let summary_entry = MemoryEntry {
            id: "sum1".to_string(),
            key: summary_key.clone(),
            content: "User mentioned they love hiking and have a dog named Biscuit.".to_string(),
            category: MemoryCategory::Conversation,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            session_id: None,
            score: None,
        };
        let memory = MockMemory::with_entries(vec![summary_entry]);

        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.7, 20);

        let prompt = builder
            .build(&session, "tell me about Biscuit", &memory)
            .await
            .unwrap();

        assert!(
            prompt.contains("## Recent conversation summary"),
            "rolling summary section must be present"
        );
        assert!(
            prompt.contains("Biscuit"),
            "summary content must appear in prompt"
        );
    }

    #[tokio::test]
    async fn build_includes_tone_guidance_footer() {
        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.0, 0);
        let memory = MockMemory::new();

        let prompt = builder.build(&session, "hey", &memory).await.unwrap();

        assert!(
            prompt.contains("## Remember"),
            "tone guidance section must be present"
        );
        assert!(
            prompt.contains("Stay in character as Zara"),
            "persona name must appear in tone guidance"
        );
        assert!(
            prompt.contains("casual and upbeat"),
            "tone string must appear in tone guidance footer"
        );
    }

    #[tokio::test]
    async fn companion_session_key_format_is_stable() {
        let session = test_session(0.0, 0);
        assert_eq!(companion_session_key(&session), "companion:user-test");
    }

    #[tokio::test]
    async fn build_with_no_memory_omits_recall_and_summary_sections() {
        let memory = MockMemory::new(); // empty

        let builder = CompanionContextBuilder::new(test_persona(), 5);
        let session = test_session(0.0, 0);

        let prompt = builder.build(&session, "hi", &memory).await.unwrap();

        assert!(
            !prompt.contains("## What you remember about this person"),
            "empty memory must omit the recall section"
        );
        assert!(
            !prompt.contains("## Recent conversation summary"),
            "missing summary must omit the summary section"
        );
    }
}
