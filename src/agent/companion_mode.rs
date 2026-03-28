//! Companion mode session management and persona configuration.
//!
//! This module manages per-user [`CompanionSession`] state and provides
//! persona selection logic for the ClawdCompanion AI companion mode.
//! Config structs defined here will be reconciled into `src/config/schema.rs`
//! during integration.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

// ── Config structs ─────────────────────────────────────────────────────────────

/// Configuration for a single companion persona character.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PersonaConfig {
    /// The persona's display name (e.g. "Zara", "Max").
    pub name: String,
    /// A brief description of the persona's background and purpose.
    pub description: String,
    /// A list of personality trait adjectives (e.g. "curious", "warm", "playful").
    pub personality_traits: Vec<String>,
    /// Tonal guidance string (e.g. "casual and upbeat").
    pub tone: String,
    /// Greeting message sent on first contact.
    pub greeting: String,
    /// Optional channel IDs or names this persona prefers (e.g. "telegram", "discord").
    #[serde(default)]
    pub channel_affinity: Vec<String>,
}

/// Top-level companion mode configuration block.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CompanionConfig {
    /// Whether companion mode is active.
    #[serde(default)]
    pub enabled: bool,
    /// Number of memories to recall per turn for context building.
    #[serde(default = "default_recall_k")]
    pub memory_recall_k: usize,
    /// Number of turns between automatic conversation summary regenerations.
    #[serde(default = "default_summary_interval")]
    pub summary_interval: u32,
    /// All available persona definitions.
    #[serde(default)]
    pub personas: Vec<PersonaConfig>,
}

fn default_recall_k() -> usize {
    5
}

fn default_summary_interval() -> u32 {
    10
}

// ── Session state ──────────────────────────────────────────────────────────────

/// Per-user companion session state, tracked in memory for the lifetime of the
/// process. State is not persisted across restarts; long-term relationship data
/// is stored in the memory backend instead.
#[derive(Debug, Clone)]
pub struct CompanionSession {
    /// Stable user identifier (e.g. Telegram user ID, Discord snowflake).
    pub user_id: String,
    /// Name of the active persona chosen for this user.
    pub persona_name: String,
    /// Total number of conversational turns completed in this session.
    pub turn_count: u32,
    /// Familiarity score in the range `[0.0, 1.0]`.
    ///
    /// Starts at `0.0` and grows by `0.01` per completed turn, capped at `1.0`.
    /// Higher values unlock warmer, more intimate response styles.
    pub familiarity_score: f64,
}

// ── CompanionMode ──────────────────────────────────────────────────────────────

/// Manages all active companion sessions and exposes persona metadata.
///
/// Thread-safe: the sessions map is protected by a [`tokio::sync::RwLock`].
pub struct CompanionMode {
    /// Companion configuration (personas, intervals, feature flag).
    pub config: CompanionConfig,
    sessions: Arc<RwLock<HashMap<String, CompanionSession>>>,
}

impl CompanionMode {
    /// Create a new [`CompanionMode`] from the supplied configuration.
    pub fn new(config: CompanionConfig) -> Self {
        Self {
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Returns `true` when companion mode is enabled in config.
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Return an existing session for `user_id`, or create one by selecting the
    /// best persona.
    ///
    /// Persona selection priority:
    /// 1. If `channel` is `Some(ch)`, prefer the first persona whose
    ///    `channel_affinity` contains `ch`.
    /// 2. Otherwise (or when no affinity match is found), use the first persona.
    ///
    /// # Panics
    /// Panics if no personas are configured — callers must guard with
    /// [`is_enabled`](Self::is_enabled) and validate config before calling.
    pub async fn get_or_create_session(
        &self,
        user_id: &str,
        channel: Option<&str>,
    ) -> CompanionSession {
        // Fast path: session already exists.
        {
            let guard = self.sessions.read().await;
            if let Some(session) = guard.get(user_id) {
                return session.clone();
            }
        }

        let persona_name = self.select_persona_name(channel);

        let session = CompanionSession {
            user_id: user_id.to_string(),
            persona_name,
            turn_count: 0,
            familiarity_score: 0.0,
        };

        let mut guard = self.sessions.write().await;
        // Handle race: another task may have inserted while we waited.
        guard
            .entry(user_id.to_string())
            .or_insert(session)
            .clone()
    }

    /// Increment `turn_count` and grow `familiarity_score` by `0.01` (capped at
    /// `1.0`) for the session identified by `user_id`.
    ///
    /// Returns `true` if a summary should be regenerated, i.e.
    /// `turn_count % summary_interval == 0` after the increment, and
    /// `summary_interval > 0`.
    ///
    /// Does nothing (returns `false`) if no session exists for `user_id`.
    pub async fn after_turn(&self, user_id: &str) -> bool {
        let mut guard = self.sessions.write().await;
        let Some(session) = guard.get_mut(user_id) else {
            return false;
        };

        session.turn_count = session.turn_count.saturating_add(1);
        session.familiarity_score = (session.familiarity_score + 0.01).min(1.0);

        let interval = self.config.summary_interval;
        interval > 0 && session.turn_count % interval == 0
    }

    /// Look up a persona by exact name match.
    ///
    /// Returns `None` if no persona with that name is configured.
    pub fn persona(&self, name: &str) -> Option<&PersonaConfig> {
        self.config.personas.iter().find(|p| p.name == name)
    }

    /// Return the first configured persona.
    ///
    /// # Panics
    /// Panics when `config.personas` is empty.
    pub fn default_persona(&self) -> &PersonaConfig {
        self.config
            .personas
            .first()
            .expect("CompanionMode requires at least one configured persona")
    }

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Select the best persona name for a new session given an optional channel.
    fn select_persona_name(&self, channel: Option<&str>) -> String {
        if let Some(ch) = channel {
            if let Some(persona) = self
                .config
                .personas
                .iter()
                .find(|p| p.channel_affinity.iter().any(|a| a == ch))
            {
                return persona.name.clone();
            }
        }
        self.default_persona().name.clone()
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn persona(name: &str, affinity: &[&str]) -> PersonaConfig {
        PersonaConfig {
            name: name.to_string(),
            description: format!("{name} description"),
            personality_traits: vec!["curious".to_string(), "warm".to_string()],
            tone: "friendly".to_string(),
            greeting: format!("Hello, I'm {name}!"),
            channel_affinity: affinity.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn config_with_personas(personas: Vec<PersonaConfig>) -> CompanionConfig {
        CompanionConfig {
            enabled: true,
            memory_recall_k: 5,
            summary_interval: 10,
            personas,
        }
    }

    #[tokio::test]
    async fn session_is_created_with_default_persona_when_no_channel() {
        let cfg = config_with_personas(vec![persona("Zara", &[]), persona("Max", &["telegram"])]);
        let mode = CompanionMode::new(cfg);

        let session = mode.get_or_create_session("user-1", None).await;

        assert_eq!(session.user_id, "user-1");
        assert_eq!(session.persona_name, "Zara");
        assert_eq!(session.turn_count, 0);
        assert_eq!(session.familiarity_score, 0.0);
    }

    #[tokio::test]
    async fn session_is_returned_unchanged_on_second_call() {
        let cfg = config_with_personas(vec![persona("Zara", &[])]);
        let mode = CompanionMode::new(cfg);

        let first = mode.get_or_create_session("user-2", None).await;
        // Simulate some progress manually before the second call.
        {
            let mut guard = mode.sessions.write().await;
            if let Some(s) = guard.get_mut("user-2") {
                s.turn_count = 3;
            }
        }
        let second = mode.get_or_create_session("user-2", None).await;

        assert_eq!(second.turn_count, 3, "existing session must be returned");
    }

    #[tokio::test]
    async fn persona_selected_by_channel_affinity() {
        let cfg = config_with_personas(vec![
            persona("Zara", &[]),
            persona("Max", &["telegram", "discord"]),
        ]);
        let mode = CompanionMode::new(cfg);

        let session = mode
            .get_or_create_session("user-3", Some("telegram"))
            .await;

        assert_eq!(session.persona_name, "Max");
    }

    #[tokio::test]
    async fn persona_falls_back_to_default_when_no_affinity_match() {
        let cfg = config_with_personas(vec![
            persona("Zara", &[]),
            persona("Max", &["telegram"]),
        ]);
        let mode = CompanionMode::new(cfg);

        let session = mode.get_or_create_session("user-4", Some("slack")).await;

        assert_eq!(
            session.persona_name, "Zara",
            "should fall back to first persona when no channel match"
        );
    }

    #[tokio::test]
    async fn after_turn_increments_count_and_familiarity() {
        let cfg = config_with_personas(vec![persona("Zara", &[])]);
        let mode = CompanionMode::new(cfg);
        mode.get_or_create_session("user-5", None).await;

        mode.after_turn("user-5").await;
        mode.after_turn("user-5").await;

        let guard = mode.sessions.read().await;
        let session = guard.get("user-5").unwrap();
        assert_eq!(session.turn_count, 2);
        assert!((session.familiarity_score - 0.02).abs() < 1e-10);
    }

    #[tokio::test]
    async fn familiarity_score_is_capped_at_one() {
        let cfg = config_with_personas(vec![persona("Zara", &[])]);
        let mode = CompanionMode::new(cfg);
        mode.get_or_create_session("user-6", None).await;

        // Drive familiarity well past 1.0.
        for _ in 0..200 {
            mode.after_turn("user-6").await;
        }

        let guard = mode.sessions.read().await;
        let session = guard.get("user-6").unwrap();
        assert_eq!(session.familiarity_score, 1.0);
    }

    #[tokio::test]
    async fn after_turn_returns_true_at_summary_interval() {
        let cfg = CompanionConfig {
            enabled: true,
            memory_recall_k: 5,
            summary_interval: 5,
            personas: vec![persona("Zara", &[])],
        };
        let mode = CompanionMode::new(cfg);
        mode.get_or_create_session("user-7", None).await;

        let mut summary_triggers = vec![];
        for _ in 0..12 {
            summary_triggers.push(mode.after_turn("user-7").await);
        }

        // Turns 5 and 10 (1-indexed count) should trigger a summary.
        assert!(summary_triggers[4], "turn 5 should trigger summary");
        assert!(summary_triggers[9], "turn 10 should trigger summary");
        // Intervening turns must not trigger.
        assert!(!summary_triggers[3], "turn 4 must not trigger summary");
        assert!(!summary_triggers[5], "turn 6 must not trigger summary");
    }

    #[tokio::test]
    async fn after_turn_for_unknown_user_returns_false() {
        let cfg = config_with_personas(vec![persona("Zara", &[])]);
        let mode = CompanionMode::new(cfg);

        let result = mode.after_turn("nonexistent-user").await;
        assert!(!result);
    }

    #[test]
    fn persona_lookup_by_name_works() {
        let cfg = config_with_personas(vec![persona("Zara", &[]), persona("Max", &["telegram"])]);
        let mode = CompanionMode::new(cfg);

        assert!(mode.persona("Zara").is_some());
        assert_eq!(mode.persona("Zara").unwrap().tone, "friendly");
        assert!(mode.persona("Unknown").is_none());
    }

    #[test]
    fn default_persona_returns_first() {
        let cfg = config_with_personas(vec![persona("Alpha", &[]), persona("Beta", &[])]);
        let mode = CompanionMode::new(cfg);
        assert_eq!(mode.default_persona().name, "Alpha");
    }

    #[test]
    fn is_enabled_reflects_config_flag() {
        let mut cfg = config_with_personas(vec![persona("Zara", &[])]);
        cfg.enabled = false;
        let mode = CompanionMode::new(cfg);
        assert!(!mode.is_enabled());
    }

    #[test]
    fn companion_config_default_values_are_sensible() {
        let cfg = CompanionConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.memory_recall_k, 5);
        assert_eq!(cfg.summary_interval, 10);
        assert!(cfg.personas.is_empty());
    }
}
