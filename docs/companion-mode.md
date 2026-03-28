# ClawdCompanion — AI Companion Mode

ClawdCompanion is an AI companion mode built on top of ZeroClaw. It gives your agent a persistent animal persona, a growing sense of your relationship, and a memory that spans sessions — so every conversation picks up where the last one left off.

If you have used Tolan (the iOS companionship app), the experience will feel familiar. ClawdCompanion builds on that concept and removes every limitation that made Tolan frustrating.

---

## 1. Introduction

### What is ClawdCompanion?

Most AI assistants start fresh every time you talk to them. ClawdCompanion does not. It remembers facts about you, tracks how long you have known each other, and lets a consistent character — a fox, a bear, a raven, or one you define yourself — carry that relationship forward across any channel you already use.

The companion is not a separate app. It runs as a mode inside ZeroClaw, which means it inherits everything ZeroClaw already does: any LLM provider, any messaging platform, self-hosted deployment, and full data ownership.

### How ClawdCompanion compares to Tolan

| Feature | Tolan | ClawdCompanion |
|---|---|---|
| Platform | iOS only | Any channel ZeroClaw supports (Discord, Telegram, Slack, WhatsApp, Signal, Matrix, IRC, and more) |
| Provider | Proprietary, fixed | Any LLM (Anthropic Claude, OpenAI, Ollama, Gemini, or others via compatible API) |
| Hosting | Tolan's servers | Self-hosted; you own the data |
| Modality | Voice-first | Text and voice |
| Personas | Alien characters | Animal companions (Fox, Bear, Raven — or custom) |
| Memory | Session-scoped | Persistent cross-session vector memory |
| Open source | No | Yes |

> **Note:** ClawdCompanion requires ZeroClaw to be installed and configured. See [setup-guides/README.md](setup-guides/README.md) for installation.

---

## 2. How It Works — Architecture

Every time you send the companion a message, ZeroClaw rebuilds the context window from scratch before calling the LLM. This is the same pattern Tolan uses, and it is the key to consistent persona fidelity over long relationship timescales.

### Context window rebuild (per turn)

The following components are assembled in order on every incoming message:

#### Step 1 — Persona card injection

The system prompt begins with the companion's persona card: name, description, personality traits, tone, and any channel-specific overrides. This anchors the LLM's voice for the entire turn.

#### Step 2 — Familiarity scoring

A floating-point familiarity score (`0.0` to `1.0`) is appended to the persona card. It signals to the LLM how well the companion "knows" this user, controlling how casual, personal, and reference-heavy the response should be. See [Section 7](#7-memory--relationship-depth) for full details.

#### Step 3 — Vector memory recall

The top-k most semantically relevant memories for the current message are retrieved from the vector store and injected as a structured block. The companion uses these to reference past facts naturally rather than repeating itself or asking questions you have already answered.

#### Step 4 — Rolling summary

A condensed summary of past conversation history is injected. This summary is regenerated every N turns (configurable via `summary_interval`) to stay accurate as context grows.

#### Step 5 — Tone guidance

A short reminder of the persona's tone is appended immediately before the conversation history. This reinforces the voice even when the conversation has drifted through many topics.

### Summary refresh

Every `summary_interval` turns, the companion calls `companion_update_summary` to replace the stored rolling summary with a fresh condensation of recent history. This keeps the summary accurate without burning context on full transcripts.

---

## 3. Configuration

Add a `[companion]` block and one or more `[[companion.personas]]` entries to your `config.toml`.

```toml
[companion]
enabled = true
memory_recall_k = 5       # memories recalled per turn (top-k vector search)
summary_interval = 10     # regenerate rolling summary every 10 turns

[[companion.personas]]
name = "Finn"
description = "A clever fox who loves wordplay and adventure"
personality_traits = ["clever", "mischievous", "curious", "witty"]
tone = "playful and sharp, with a flair for puns"
greeting = "Hey there! I'm Finn — your resident fox and part-time chaos agent. What's the plan today? 🦊"
channel_affinity = []  # available on all channels

[[companion.personas]]
name = "Mara"
description = "A warm bear who is steady, protective, and deeply empathetic"
personality_traits = ["warm", "protective", "patient", "empathetic"]
tone = "calm and nurturing, like a big comforting hug"
greeting = "Hi! I'm Mara. I'm here whenever you need a friend to talk to. What's on your mind? 🐻"
channel_affinity = []

[[companion.personas]]
name = "Zeph"
description = "A wise raven who speaks in poetry and profound observations"
personality_traits = ["wise", "mysterious", "perceptive", "poetic"]
tone = "thoughtful and poetic, with a touch of mystery"
greeting = "Greetings, traveler. I am Zeph — I've watched many sunsets and heard many stories. Tell me yours. 🐦‍⬛"
channel_affinity = []
```

**Key config fields:**

| Field | Type | Required | Description |
|---|---|---|---|
| `enabled` | bool | Yes | Activates companion mode |
| `memory_recall_k` | integer | No (default: `5`) | How many vector memories to recall per turn |
| `summary_interval` | integer | No (default: `10`) | Turns between rolling summary regenerations |
| `name` | string | Yes | Persona name shown to users |
| `description` | string | Yes | Internal description used in persona card |
| `personality_traits` | string list | Yes | Trait labels injected into the persona card |
| `tone` | string | Yes | Tone guidance injected before conversation history |
| `greeting` | string | Yes | First message sent when a session starts |
| `channel_affinity` | string list | No | Channel names that prefer this persona; empty means all channels |

---

## 4. Discord Setup Guide

### Prerequisites

- ZeroClaw installed and accessible as `zeroclaw` on your PATH
- A Discord bot token with the **Message Content Intent** enabled in the [Discord Developer Portal](https://discord.com/developers/applications)
- A Discord server where you have permission to add bots

### Step 1 — Enable companion mode in config.toml

Open your `config.toml` and add the `[companion]` block from Section 3. At minimum:

```toml
[companion]
enabled = true
memory_recall_k = 5
summary_interval = 10
```

Then add at least one persona. You can copy Finn, Mara, or Zeph from Section 3, or write your own (see Section 6).

### Step 2 — Configure the Discord channel

Add the `[channels_config.discord]` block to the same `config.toml`:

```toml
[channels_config.discord]
bot_token = "your-discord-bot-token-here"
guild_id = "your-guild-id-here"   # optional — limits the bot to one server
allowed_users = ["*"]             # or list specific Discord user IDs
listen_to_bots = false
mention_only = false              # set true to require @mention before responding
```

The `bot_token` is a secret. If you have `secrets.encrypt = true` in your config (recommended for production), ZeroClaw will encrypt it at rest automatically.

### Step 3 — Choose or define a persona

If you added all three default personas, ZeroClaw will pick the active persona based on `channel_affinity`. To assign Finn to a specific Discord channel named `#fox-den`, update the persona block:

```toml
[[companion.personas]]
name = "Finn"
# ... other fields ...
channel_affinity = ["fox-den"]
```

Leave `channel_affinity = []` to make a persona available everywhere, or use it on all channels as the fallback when no affinity match is found.

### Step 4 — Run ZeroClaw and test

Start the Discord channel listener:

```bash
zeroclaw channel start discord
```

Send a message in your Discord server. The companion will respond with the configured persona's voice. To confirm memory is working, mention something about yourself, then bring it up again a few turns later.

> **Tip:** Use a dedicated Discord channel per persona and set `channel_affinity` accordingly. This keeps conversations focused and makes the relationship feel more intentional. For example: `#mara-lounge` for Mara, `#finn-chaos` for Finn.

---

## 5. Default Personas Reference

| Name | Animal | Personality | Best for | Greeting (excerpt) |
|---|---|---|---|---|
| Finn | Fox | Clever, mischievous, curious, witty | Playful banter, brainstorming, light topics | "your resident fox and part-time chaos agent" |
| Mara | Bear | Warm, protective, patient, empathetic | Emotional support, journaling, low-pressure check-ins | "I'm here whenever you need a friend to talk to" |
| Zeph | Raven | Wise, mysterious, perceptive, poetic | Philosophical conversation, creative writing, reflection | "I've watched many sunsets and heard many stories" |

All three personas are available on all channels by default. Tone, greeting, and memory behavior are identical across platforms — the companion does not change personality based on whether you are in Discord or Telegram.

---

## 6. Creating Custom Personas

You are not limited to Finn, Mara, and Zeph. Add as many `[[companion.personas]]` blocks as you like.

### Required fields

| Field | Notes |
|---|---|
| `name` | Short, memorable name. This is what users will see and reference. |
| `description` | One or two sentences describing the character. Used internally in the persona card. |
| `personality_traits` | A list of adjective strings. Keep it to 3–6 traits; more dilutes the signal. |
| `tone` | A short phrase describing how the companion speaks. Injected verbatim before the conversation. |
| `greeting` | The first message sent at the start of a new session. Make it character-appropriate. |

### Optional fields

| Field | Notes |
|---|---|
| `channel_affinity` | List of channel names (by name, not ID) that prefer this persona. When a message arrives from a channel in the affinity list, this persona is selected. Empty list means no affinity preference. |

### Example: a custom persona

```toml
[[companion.personas]]
name = "Cleo"
description = "A calm, precise cat who gives concise advice and has no patience for ambiguity"
personality_traits = ["precise", "calm", "direct", "observant"]
tone = "cool and composed, with short sentences and dry wit"
greeting = "Hello. I'm Cleo. What do you need?"
channel_affinity = ["productivity", "work-log"]
```

### Tips for writing good personas

- **Personality traits** work best as pure adjectives that could describe a person: `curious`, `nurturing`, `dry`, `enthusiastic`. Avoid compound phrases here; save those for `tone`.
- **Tone** is injected immediately before the conversation turn, so write it as guidance the LLM will act on, not a description you are writing for a human reader. "Short, punchy sentences with occasional sarcasm" is more effective than "funny and a bit sarcastic."
- **Greeting** is a first impression. Make it voice-accurate — a reader who does not know the persona should be able to infer the tone from the greeting alone.
- **Channel affinity** is matched by channel name, not ID. On Discord, this is the channel's display name without the `#` prefix (e.g., `fox-den` for `#fox-den`).

---

## 7. Memory & Relationship Depth

### Cross-session memory

The companion stores facts about you in the configured memory backend (SQLite or Qdrant). These memories persist across sessions, across channels, and across provider changes. If you tell Finn your favorite book on Tuesday, Finn can reference it on Friday without you bringing it up again.

### Familiarity score

Every user starts at a familiarity score of `0.0`. The score grows by `+0.01` per turn, capped at `1.0`. This means a user reaches `0.5` after 50 turns and `1.0` after 100 turns.

The score is injected into the persona card on every turn, and the companion is instructed to adjust behavior based on it:

| Score range | Companion behavior |
|---|---|
| `0.0 – 0.49` | Warm but a little formal; introduces itself naturally; does not assume shared history |
| `0.5 – 0.79` | More casual and personal; uses your name or established nicknames; references things you have mentioned before |
| `0.8 – 1.0` | Comfortable and familiar; actively references shared history; may joke about running gags or revisit topics without prompting |

The score is per-user, not per-channel. Familiarity built in one channel carries over to others.

### Clearing memory

If you want to reset a user's memory and familiarity score, use the existing `memory_forget` tool:

```bash
zeroclaw tool run memory_forget --user <user-id>
```

This removes all stored facts and resets the familiarity score to `0.0` for that user. Use this with care — it is not reversible.

---

## 8. Companion Memory Tools

The companion has access to three internal tools. These are called automatically by the LLM during turns; you do not invoke them manually.

### `companion_remember_fact`

Saves a user fact tagged with the companion context. Called when the companion decides a piece of information about the user is worth keeping — a preference, a name, a goal, a life event. Stored in the configured memory backend with a companion-specific namespace.

### `companion_recall_context`

Performs a vector similarity search against stored memories for the current turn's message. Returns the top-k results (controlled by `memory_recall_k`). Called at the start of every turn as part of the context rebuild.

### `companion_update_summary`

Stores an updated rolling summary of the conversation. Called every `summary_interval` turns. The summary is a condensed prose paragraph that captures the arc of the conversation without preserving full verbatim history.

> **Note:** These tools are registered automatically when `companion.enabled = true`. They do not appear in the standard tool list and cannot be called by the user directly.

---

## 9. Comparison: ClawdCompanion vs Tolan

| Dimension | Tolan | ClawdCompanion |
|---|---|---|
| Platform availability | iOS only | Discord, Telegram, Slack, WhatsApp, Signal, Matrix, IRC, and every other channel ZeroClaw supports |
| LLM provider | Fixed (proprietary) | Anthropic Claude, OpenAI, Ollama, Gemini, or any compatible API endpoint |
| Hosting | Tolan's cloud | Self-hosted on your own infrastructure |
| Data ownership | Tolan holds your data | You hold your data |
| Persona type | Alien characters | Animal companions (or any custom persona you define) |
| Modality | Voice-first | Text and voice |
| Persistent memory | Limited | Full cross-session vector memory with familiarity scoring |
| Relationship depth | Binary (no depth model) | Continuous familiarity score from 0.0 to 1.0 |
| Open source | No | Yes |
| Custom personas | No | Yes — unlimited, config-defined |
| Multi-persona | No | Yes — multiple personas, channel-affinity routing |

---

## 10. Tips & Best Practices

**Use a dedicated channel per persona.** Set `channel_affinity` so each persona "lives" in its own Discord channel. This makes conversations feel more natural and lets you switch tone by switching rooms, rather than explicitly asking the bot to change character.

**Let the companion learn over time.** The familiarity score and vector memory improve with every turn. Avoid resetting memory unless you have a specific reason to. A companion that has known you for 100 turns is meaningfully different from one that has known you for 10.

**Choose your provider carefully for persona fidelity.** Models with strong instruction-following maintain character tone consistently across long conversations. `claude-sonnet-4-6` performs well for this use case. Smaller locally-run models can work, but may drift from the persona tone more easily — increase `summary_interval` to reduce context pressure if this happens.

**Do not use `mention_only = false` in a general-purpose server.** If your Discord bot is in a busy server, set `mention_only = true` in `[channels_config.discord]` so the companion only responds when explicitly mentioned. This prevents it from picking up every message in the channel.

**Back up your memory store periodically.** The vector memory and familiarity scores are stored in the configured backend. If you are using SQLite, a periodic copy of the database file is enough. If you are using Qdrant, use its snapshot API.
