# Changelog

All notable changes to Gamekeeper will be documented in this file.

## [Unreleased]

### Added
- **Write mode for Steam collections**: choose **Add new games only** (default) or **Replace with Gamekeeper's sorting**. Add-new never removes a game and files a title only when none of the four managed collections already holds it, so hand-sorted collections survive a write. Replace is the old behavior and is now opt-in
- **Couch / TV filter**: a "Play style" toggle in the sidebar (and next to the search bar) that narrows the library to gamepad-friendly games, with options for partial controller support and split-screen only. Derived from Steam's own store categories, so it needs no extra fetch and works offline
- **Controller badges** on game cards, plus a "Couch & TV" panel in game details showing controller support, split screen, Remote Play on TV, and Remote Play Together
- **Optional "Controller Friendly" Steam collection**: opt in when writing to Steam and your gamepad-ready games show up as their own collection in Big Picture mode on the TV. VR-only titles and non-games are excluded
- "Write to Steam" now shows a confirmation step listing what it will write, instead of writing immediately when only one Steam account exists. The success screen reports the collection names and game counts the backend actually wrote, rather than restating what the UI asked for

### Changed
- **Steam collections lost the `SBO:` prefix.** Gamekeeper now writes `Completed`, `In Progress`, `Endless/Multiplayer`, `Not a Game`, and `Controller Friendly`, updating collections with those names in place instead of creating a duplicate prefixed set beside them. Leftover `SBO:` collections are deleted on the next write. Collections named anything else are never touched
- Newly created collections carry the same conflict-resolution fields Steam stamps on its own user collections

## [4.0.0] - 2026-09-01

The Taste Engine release. Gamekeeper now knows your taste — fully offline, no GPU, no model download required.

### Added
- **Discover view**: ranked recommendations from a bundled catalog of ~60,000 Steam games you *don't* own, scored against your taste in ~18ms. Filters for rating, review count, release window, and owned games
- **Taste Profile view**: "what your library says about you" — tag-affinity signature, the anchor games that define it, bounce patterns, and (with an AI model installed) a written profile from your local LLM
- **Taste engine core**: 256-dim embedding index (potion-base-8M via model2vec, pure Rust) with playtime/completion/recency-weighted taste vector, computed from your real play history in ~3ms
- **Anti-recommendations**: detects games you bounced off (low playtime, long-abandoned) and warns you before similar purchases — "you've bounced off 4 competitive FPS games — this looks like one"
- **Wishlist scoring**: your Steam wishlist ranked by taste match with bounce warnings (works keyless for public profiles)
- **"More like this"** on every game: franchise-filtered, diversity-ranked similar games with cross-genre finds badged "unexpected"
- **Taste-fit line** in game details: match percentage, shared tags, and your closest library neighbors
- **AI model tiers**: choose None / Qwen3.5-4B (2.7 GB) / Qwen3-8B (5 GB) / Qwen2.5-14B (15.7 GB), with hardware-based recommendation (VRAM detection via nvidia-smi with DXGI fallback). The entire app is fully functional with no model installed
- **Catalog build pipeline** (`scripts/build_catalog.py`): reproducible artifact builds from the FronkonGames Steam dataset with golden-neighbor quality checks
- Library sync now captures last-played time and two-week playtime (powers bounce detection)
- Store cache now keeps descriptions, Metacritic scores, review counts, developers, and release dates (versioned format with automatic background backfill)

### Changed
- AI chat candidates are now taste-ranked (blended with your message's semantics) instead of Steam API order — the model sees your 40 *best-matching* backlog games, not the first 40 alphabetically
- With no AI model installed, chat returns deterministic taste-ranked picks instead of canned suggestions
- Settings AI section rebuilt around model tiers with per-tier download/activate/delete
- llama-server errors now logged to `llama-server.log` (GPU OOM and template failures were previously invisible)

### Fixed
- Downloading an AI model no longer deletes other models on disk
- Existing 14B installs are auto-adopted as the "Max" tier on upgrade — no re-download
- Playtime display no longer requires a fresh sync after app restart (cold-start cache hydration)
- Qwen3-family `<think>` blocks are stripped defensively from chat responses

## [3.2.1] - 2026-04-04

### Changed
- New "Modern Curator" design system: teal accent (#14b8a6), Space Grotesk/Geist typography, neutral grays
- New app icon: teal shield with controller motif (AI-generated with Flux.1 Dev)
- Updated README with screenshots and repo metadata

## [3.2.0] - 2026-03-28

### Added
- HowLongToBeat integration: completion time estimates for your entire Steam library, fetched automatically after each sync
- "Short games" filter: toggle + adjustable max hours slider to find games that fit your schedule
- Game detail panel shows HLTB completion times (Main Story, Main + Extra, Completionist) with ~Time Left calculation based on your actual playtime
- AI chat is now time-aware: ask "something short" or "I have 2 hours" and it prioritizes games by estimated time remaining
- HLTB data cached locally with incremental background fetch, progress indicator, and ETA
- Filter settings persist across app restarts

### Changed
- AI model upgraded to Qwen2.5-14B-Instruct Q8_0 (~16 GB) for faster inference and higher quality responses
- Follow-up messages in AI chat use a trimmed candidate list (15 instead of 40) for snappier responses
- Settings panel now includes Completion Times section with HLTB attribution and explanation

### Fixed
- "~Time Left" now shows actual remaining time (HLTB estimate minus your playtime) instead of raw total
- Title normalization only strips edition suffixes at the end of names, preventing false truncation
- Separate cancel flag for HLTB fetch so cancelling Steam sync doesn't permanently break completion time fetching
- Concurrent HLTB fetch guard prevents double requests if you sync twice quickly
- HLTB cache poll only runs while fetch is in progress (was polling every 10s forever)
- Mutex locks in background tasks use graceful error handling instead of panicking
- Cache save errors are logged instead of silently swallowed

## [3.1.0] - 2026-03-27

### Added
- Bundled AI chat: "What should I play next?" powered by local Qwen model
- Ambiguity assistant for medium-confidence classifications
- GPU/CPU auto-detection with CUDA support
- Custom app icon (gamepad + checkmark)
- Dark "Steam-meets-Netflix" theme with animations

### Changed
- Full rewrite from Python to Tauri + React + TypeScript + Rust
- Single binary, no external dependencies required
