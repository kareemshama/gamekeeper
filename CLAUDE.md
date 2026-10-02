# CLAUDE.md — Steam Backlog Organizer

Guidance for Claude Code and collaborators working in this repository. **Read this before large changes.**

---

## Project overview

**Steam Backlog Organizer** — Desktop tool (Python CLI + CustomTkinter GUI today) that categorizes a user’s Steam library into **four collections**:

| Collection | Meaning |
|------------|---------|
| **Completed** | Finished main story/campaign |
| **In Progress / Backlog** | Completable games not finished |
| **Endless** | No real ending (multiplayer, sandbox, strategy, etc.) |
| **Not a Game** | Demos, tools, soundtracks, servers, etc. |

**Output:** Writes collections into Steam’s **local cloud storage** with correct sync metadata so collections **sync across machines**.

**Classification:** **Rule-based** (Steam Web API + Store API + achievements + playtime + heuristics). **No cloud AI API required** for core behavior (see history below).

---

## Product history (context for rebuild)

- **v0.x:** AI-only (Claude API) — cost + requiring user API keys was a problem.
- **v1.x:** Hybrid — rules for most games; AI optional for ambiguous titles.
- **v2.x (current):** **Pure rules** — AI removed; expanded rule set (~14 priority rules); free Steam API key only.

**User intent now:** Rebuild the **frontend** in **Tauri + React + TypeScript** (same modern stack as `debt-planner-local`) for **look, feel, and performance**. Optional **local LLM** (bundled llama-server + Qwen2.5-3B) for **assistive** features only — no external dependency required.

---

## Commands (current Python app)

```powershell
cd steam-backlog-organizer
python -m venv venv
.\venv\Scripts\Activate.ps1

pip install requests rich customtkinter pillow

# Run
python organizer.py              # CLI
python gui.py                    # GUI
python organizer.py --setup      # reconfigure API key / Steam ID
python organizer.py --override   # manual category overrides

# Windows executable (PyInstaller)
pip install pyinstaller
python build.py                  # dist/SteamBacklogOrganizer.exe (GUI)
python build.py --cli            # dist/SteamBacklogOrganizer-CLI.exe
```

**Tests:** No automated test suite today — add golden JSON / parity tests when extracting or porting logic.

---

## Architecture (current)

### Single-file core: `organizer.py`

Major responsibilities:

| Area | Details |
|------|---------|
| **Config** | Steam API key + Steam ID. **Path (Windows):** `%APPDATA%\SteamBacklogOrganizer\config\settings.json`. Priority: env vars → saved file → interactive prompt (`run_setup`, `get_config`). |
| **Data dirs** | Base: `%APPDATA%\SteamBacklogOrganizer` (else `~/.steam-backlog-organizer`). **Config:** `config/`. **Cache:** `cache/` (not `.cache` at repo root — cache lives under AppData). |
| **Key files** | `cache/library.json` (24h library cache), `cache/store_details.json` (permanent store cache), `cache/classifications_final.json` (saved classifications), `config/overrides.json` (user overrides — **always win**), `cache/images/`, `cache/tag_mappings.json`. |
| **Steam Web API** | Owned games, playtime, per-game achievements. Rate limiting (~0.5s between calls). Timeouts + error handling. |
| **Steam Store API** | `fetch_store_details` / batch — genres, categories, type (demo/tool/DLC). Rate limiting (~0.3s). |
| **Classification** | `classify_by_rules()` — priority rules (store type, name patterns, story achievements, achievement %, multiplayer/MMO, genres, playtime heuristics, fallbacks). `classify_all_games()` — **overrides → saved → rules** for new/changed games. |
| **Steam collections I/O** | `write_collections_to_steam` / related — reads/writes `cloud-storage-namespace-1.json` under Steam **userdata**, and updates **sync companion files** (`cloud-storage-namespace-1.modified.json`, `cloud-storage-namespaces.json`) so Steam cloud picks up changes. **`STEAM_BASE`** default: `C:/Program Files (x86)/Steam`. |
| **Steam process** | `is_steam_running()`, `wait_for_steam_closed()` — user must **close Steam** before writing collections. |
| **CLI UX** | Rich tables, progress, `run_override_menu`, JSON export helpers. |

### GUI: `gui.py`

- **CustomTkinter** — large single module; Simple vs Detailed views, threading, image loading (PIL), bundled fonts on Windows.
- **Pain points driving rebuild:** Feels dated, clunky, some jank; performance of large lists / redraws; hard to match a modern design system.

### Build: `build.py`

- PyInstaller one-file `.exe`; bundles assets (e.g. fonts) via spec.

---

## Key design decisions (do not break)

1. **Overrides always beat rules** — `overrides.json` is the source of truth for corrections.
2. **Saved classifications persist** — Only **new** games need full classification on later runs (see `classifications_final.json`).
3. **Four categories only** — Same enum semantics everywhere (COMPLETED, IN_PROGRESS, ENDLESS, NOT_A_GAME — verify exact strings in `organizer.py`).
4. **Steam cloud trio** — Writing collections is not only the namespace JSON; **sync metadata files** must stay consistent or cloud sync breaks.
5. **Steam closed** — Document and enforce in UI before write.
6. **Rule engine stays auditable** — Prefer deterministic logic for “official” category; any LLM is **assistive**, not silent authority.

---

## Rebuild: Tauri + React + TypeScript (Approach B — full port)

**Decision:** Full port to **Tauri v2 + React 19 + TypeScript + Vite + Tailwind** (same stack as Thaw / `debt-planner-local`). No Python sidecar — single binary, clean break from the prototype.

**Goal:** Same modern app shell and patterns as `debt-planner-local`. Reuse design tokens and conventions where sensible.

| Layer | Language | Responsibilities |
|-------|----------|-----------------|
| **Tauri backend** | **Rust** | Steam Web API + Store API calls, rate limiting, rule-based classifier, Steam collection file I/O, config/cache file management, bundled LLM (llama-server) |
| **Frontend** | **React + TypeScript** | UI, state management, game grid, chat panel, settings |
| **Shared types** | **TypeScript** (`packages/core-types` or inline) | Game, classification, config types |

---

### Parity testing strategy

**Goal:** Prove the Rust rule engine produces **identical classifications** to the Python version before retiring it.

#### Step 1 — Snapshot Python outputs (do this BEFORE building)

Run the current Python app against real data and export golden fixtures:

1. **`fixtures/golden_library.json`** — Raw API response from `get_owned_games` (cached `library.json`).
2. **`fixtures/golden_store_details.json`** — Cached store details for all owned games.
3. **`fixtures/golden_classifications.json`** — Output of `classify_all_games()` with per-game `{ appid, title, category, reason, confidence }`.
4. **`fixtures/golden_overrides.json`** — Current overrides file (if any).
5. **`fixtures/golden_collections_output.json`** — The final collections object that would be written to Steam.

Add a small Python script (`scripts/export_fixtures.py`) that runs `organizer.py` functions and dumps these files. This only needs to run once (or on-demand to refresh).

#### Step 2 — Rust parity tests

Write Rust integration tests that:

1. Load `golden_library.json` + `golden_store_details.json` as input (skip live API calls).
2. Run the Rust rule engine's `classify_all_games()`.
3. Diff output against `golden_classifications.json` — **every game must match category**. Any mismatches must be triaged: fix the Rust logic if it's a real bug, or add to an explicit accepted-diffs list with justification (e.g. float rounding, tie-break order).
4. If any diffs, print `{ appid, title, python_category, rust_category, python_reason, rust_reason }` for easy debugging.
5. Optionally: test the collections output format matches `golden_collections_output.json`.

#### Step 3 — Side-by-side mode (optional, for ongoing validation)

If useful during development, add a Tauri command `compare_with_python` that:

1. Shells out to `python organizer.py --export-json` (a new flag we add to the Python version).
2. Runs the Rust classifier on the same input.
3. Returns a diff report to the UI.

This is a dev-only tool — remove before shipping.

---

### Build phases

#### Phase 0 — Golden fixtures + Python export script ✅
- [x] Write `scripts/export_fixtures.py` — calls organizer functions, dumps all golden files to `fixtures/`.
- [x] Run against Kareem's real library (569 games), commit fixtures.

#### Phase 1 — Tauri scaffold + Rust core ✅
- [x] Init Tauri v2 + React + Vite + Tailwind project (npm workspaces monorepo like Thaw).
- [x] Rust module: **config** — read/write `%APPDATA%\SteamBacklogOrganizer\config\settings.json` (same paths as Python, backward compatible).
- [x] Rust module: **steam_api** — `get_owned_games()`, `get_player_achievements()`, `fetch_store_details()` with rate limiting and timeouts.
- [x] Rust module: **classifier** — port all ~14 priority rules from `classify_by_rules()`. Match Python logic exactly.
- [x] Rust module: **cache** — read/write `library.json`, `store_details.json`, `classifications_final.json`, `overrides.json` (same formats).
- [x] **Parity tests** — 569/569 games match Python classifications exactly (0 mismatches).
- **Scope note:** Recommendation/taste functions (`build_taste_profile`, `recommend_for_you`, `recommend_game`, store search) are **not** ported in Phase 1. These are deferred to Phase 4 where they serve as deterministic candidate filters for the chat panel.

#### Phase 2 — Steam collection I/O ✅
- [x] Rust module: **collections** — read/write `cloud-storage-namespace-1.json` + sync companion files.
- [x] Steam process detection (`is_steam_running`) — platform-specific (Windows first, Linux fallback).
- [x] Steam-closed guard: block writes + show warning in UI if Steam is running.
- [x] "Write to Steam" UI modal with account selection, status feedback, and error handling.

#### Phase 3 — Frontend (core UI) ✅
- [x] Setup/onboarding screen — Steam API key + Steam ID input, validation.
- [x] Game library grid — card grid with game header images, category badges, search/filter.
- [x] Category views — sidebar filter by All / Completed / In Progress / Endless / Not a Game with counts.
- [x] Manual override UI — detail modal to view classification reason and change category (writes to overrides).
- [x] Sync flow — fetch → classify → review → write to Steam, with async progress indicators at each stage.
- [ ] Virtualized grid for 500+ game performance (currently renders all cards — works but could be smoother).
- [x] Settings panel — config management, data info, about section.

#### Phase 4 — Local LLM integration (bundled llama-server) ✅
- [x] Rust module: **llm** — bundled llama-server (llama.cpp) + Qwen2.5-3B-Instruct GGUF model, auto-downloaded on first use (~2 GB one-time). No external dependency (Ollama removed).
- [x] Ambiguity assistant — “Ask AI for a second opinion” on MEDIUM-confidence games in detail modal.
- [x] “What should I play next?” chat panel — retrieve-then-rank flow (deterministic filter → LLM picks top 1–3), suggestion chips, conversational UI.
- [x] GPU/CPU detection + CUDA auto-download (nvidia-smi probe, GPU toggle in settings).
- [x] Fallback: deterministic recommendations when AI not set up or unavailable.
- [x] AI settings in Settings panel — one-click download, GPU toggle, setup status indicator.
- [x] Export classifications as JSON from Settings.

#### Phase 5 — Polish + distribution ✅
- [x] Dark “Steam-meets-Netflix” theme — custom scrollbar, Inter font, Steam color palette.
- [x] Animations — fadeIn modals, scaleIn dialogs, slideUp chat panel, card hover lift.
- [x] README updated for v3 — new install instructions, features list, architecture link.
- [x] Python GUI/CLI retired in README — points users to Tauri app, `organizer.py` archived.
- [ ] Tauri build + Windows installer — ready to run `npx tauri build` for UAT.

---

### Rebuild checklist (cross-cutting)

- [ ] Parity: same config paths — `%APPDATA%\SteamBacklogOrganizer` (backward compatible, existing users keep their data).
- [ ] Parity: classifications + overrides file formats unchanged.
- [ ] Parity: target 100% classification match against golden fixtures before shipping. Any mismatches must be listed and either fixed or explicitly accepted (float rounding, tie-break ordering, and time-dependent cache can cause legitimate diffs).
- [ ] Virtualized / paginated game grid (performance for 500+ game libraries).
- [ ] Clear async progress (fetch → classify → write) — never block UI thread.
- [ ] Steam-closed guard + user-visible warnings.
- [ ] Export/import JSON for debugging and support.
- [ ] Optional: **”What should I play next?”** chat panel (Phase 4).

---

## Design System

Always read DESIGN.md before making any visual or UI decisions.
All font choices, colors, spacing, and aesthetic direction are defined there.
Do not deviate without explicit user approval.

- **Aesthetic:** Modern Curator (clean, structured, premium)
- **Fonts:** Space Grotesk (display), Geist (body), Geist Mono (data)
- **Accent:** #14b8a6 (teal, distinct from Steam blue and SaveForge amber)
- **Tailwind prefix:** `steam-` for custom colors (legacy naming), `font-display` / `font-mono` for fonts
- **Layout:** Card-based browsing, responsive grid, sidebar + content
- **Framework:** Tauri + React (not CustomTkinter for new work)

---

## Optional: local LLM enhancements (bundled llama-server)

**Constraints:** No Anthropic/OpenAI **required** for core flows. Local model (Qwen2.5-3B-Instruct via bundled llama-server) runs on user machine. **Rules + overrides remain canonical** unless product explicitly opts into “AI suggestion” mode.

**Inference hardware (product default):** Prefer running the local LLM on a **discrete GPU** (NVIDIA/AMD where supported) for acceptable latency. **Automatically fall back to CPU** when no discrete GPU is available, drivers/runtime are missing, or GPU inference fails — the app must remain usable (slower but functional). Expose optional settings: **“Use GPU when available”** (default **on**), and **“CPU only”** for troubleshooting. Exact behavior depends on the local runtime (e.g. Ollama) and installed drivers; surface a short **first-run or settings** note when running on CPU so expectations are clear.

**High-value, low-risk uses:**

1. **Ambiguity assistant** — For games the rules mark **low confidence** or **tie-break**, call local LLM with **structured context only** (title, genres, achievement summary, playtime) → **suggested** category + short rationale. User **confirms** before treating as override.
2. **Natural language explanations** — “Why might this be Endless?” using the same structured facts (no need to send full library).
3. **Bulk review helper** — Summarize “here are 12 games you might want to override” with reasons (read-only suggestions).
4. **Achievement / store text snippets** — LLM summarizes long achievement lists into tags for display (cosmetic / UX), not sole classifier.
5. **Chat: “What should I play next?”** — Conversational assistant that answers **only from local data** (see subsection below). Does **not** replace classification; **read-only** recommendations unless the user explicitly applies an action elsewhere in the UI.

### Chat-style “What should I play next?” (local data only)

**Intent:** A **chat panel** in the Tauri UI where the user asks things like *“What should I play for 90 minutes?”*, *“Something cozy I haven’t started”*, or *“Pick from my backlog”* — and the app responds with **1–3 concrete picks** plus short rationale, **grounded in data already on disk** (not live web unless the app already fetched it into cache).

**Data the assistant may reference (no secrets):**

- Cached library + playtime (`library.json` / in-memory equivalent after sync).
- Saved classifications + reasons (`classifications_final.json`).
- User overrides (`overrides.json`).
- Store metadata from `store_details.json` (genres, tags, categories).
- Optional: existing **deterministic** helpers in `organizer.py` (`build_taste_profile`, `recommend_for_you`, `recommend_game`, store search) to **narrow candidates** before any LLM call.

**Flow (recommended):**

1. **Parse intent** from the user message (or pass message + a **small structured profile** to the local model).
2. **Retrieve candidates deterministically** — filter/rank with rules + taste profile to a **short list** (e.g. 15–40 titles), not the full library.
3. **Local LLM** chooses **top 1–3** and writes **short “why”** text, optionally in **strict JSON** (`[{ "appid", "title", "reason" }]`).
4. **UI** shows picks as cards with links to library detail; **no automatic writes** to Steam collections from chat.

**Guards:**

- If local LLM is disabled or unreachable, fall back to **deterministic** recommendations only (same signals, no prose).
- **Never** put Steam Web API keys or session tokens in prompts — only **already-fetched** game metadata the app stores locally.
- **Avoid** pasting 500+ games into one prompt; **retrieve-then-rank** keeps latency and quality sane.

**Rebuild note:** Add an optional **Assistant / Play next** view or slide-over panel; wire settings (AI setup, GPU toggle) consistently with other local-LLM features.

**Implementation notes:**

- Use **strict JSON schema** output; validate; on parse failure fall back to rules-only.
- **Never** send API keys to the model; only public Steam data already on disk.
- Settings: **Download AI model** button (one-time ~2 GB), **GPU toggle**, setup status indicator.
- **GPU vs CPU:** Default to **discrete GPU** for inference when present; **CPU is the backup** path. Detect or query runtime capability where possible; never hard-require a GPU. If the user has no discrete graphics, do not show errors — run on CPU and optionally show “Running on CPU (slower)” in settings or the assistant panel.

**Avoid:** Sending entire 500+ game libraries to the model every run (cost/latency) — batch or filter like the old hybrid approach; the chat feature should use the same **retrieve-then-rank** pattern.

---

## Files to read first

| File | Why |
|------|-----|
| `organizer.py` | All business logic, paths, Steam I/O, rules |
| `gui.py` | Current UX flows to replicate |
| `build.py` / `*.spec` | Packaging expectations |
| `README.md` | User-facing behavior and version history |
| **`docs/HLTB_SHORT_GAMES_PLAN.md`** | **Specced feature:** HowLongToBeat integration + “Short games only” filter — **full implementation handoff** (paste or attach for Claude) |

---

## Specced feature: HowLongToBeat + “Short games only” filter

**Canonical spec (implement from this file):** [`docs/HLTB_SHORT_GAMES_PLAN.md`](docs/HLTB_SHORT_GAMES_PLAN.md)

**Goal:** Pull community playtime estimates from **HowLongToBeat** (unofficial API / crate / HTTP), cache per Steam `appid`, and add a **“Short games only”** filter (main story ≤ configurable hours) so users can answer “Do I have time to start this?” — composed with existing category filters and search.

**Constraints:** HLTB has **no official API**; integration can break if the site changes. Use **disk cache** (`hltb_data.json`), clear **attribution** in UI, and a **Settings** note. Keep HLTB data in a **parallel cache**; do not embed in `Classification` / `classifications_final.json` for v1.

**Backend (Tauri/Rust):** New `hltb` module, `AppState.hltb_cache`, `load_hltb_cache` / `save_hltb_cache`, commands `fetch_hltb_data` + `get_hltb_cache`, rate limits + `sync-progress` + cancel — details and checklist are in the doc above.

**Frontend:** Types/invokes in `commands.ts`, load cache on ready, optional HLTB fetch after sync, toggle + max hours + “include games without HLTB data” in `GameGrid`, HLTB line in `GameDetail`, Settings copy + link to https://howlongtobeat.com/

When asked to build this feature, **read `docs/HLTB_SHORT_GAMES_PLAN.md` first** and implement against that document.

---

## Related project (stack reference)

- **`C:\Users\Kareem\projects\debt-planner-local`** — Tauri + React + TS patterns, `CLAUDE.md` handoff style, local optional LLM for extraction. Reuse **design tokens** and **app shell conventions** where sensible.

---

## Open questions for the Tauri rewrite

1. ~~**Distribution:** One installer with embedded Python vs pure TS/Rust long-term?~~ **Decided:** Approach B — full Rust/TS port, single Tauri binary. No embedded Python in release builds. Python repo kept as archived reference + fixture export tool only.
2. **macOS/Linux:** Steam paths and `userdata` differ — scope v1 Windows-only or cross-platform?
3. ~~**Feature parity:** CLI-only power-user mode in Tauri or GUI-only v1?~~ **Decided:** GUI-only for v3. Python CLI remains as archived reference.

---

## v4.0 Taste Engine (shipped in `feature/taste-engine`)

Embedding-based taste engine — offline, zero GPU, zero model download required. Plan: `~/.claude/plans/glimmering-zooming-nautilus.md`.

**Architecture:**
- **Catalog artifact** (`app/src-tauri/resources/catalog.gkc`, ~23 MB, gitignored): ~60k Steam games, 256-dim int8 embeddings + gzipped metadata. Built by `scripts/build_catalog.py` from the **Kaggle** FronkonGames dataset (`games.json` — the HF parquet mirror has an EMPTY tags column, build aborts if tag coverage <50%). Format spec `GKC1` mirrored by `src/catalog.rs`; committed test fixture `fixtures/catalog_mini.gkc` + golden neighbors keep Python-writer/Rust-reader parity.
- **Embeddings**: potion-base-8M via `model2vec-rs` (pure Rust, no ONNX). Runtime embedder (`src/embed.rs`) MUST match the builder's composite text format — `fixtures/embed_parity.json` test pins the cross-language contract.
- **Taste math** (`src/taste.rs`): playtime/completion/recency-weighted taste vector; bounce/abandon detection (needs `rtime_last_played` — post-v4 sync data); anti-clusters with engagement normalization; retrieve-then-rank scoring (0.75 sim + 0.25 Bayesian review quality − anti-penalty); franchise-filtered MMR "more like this". Formulas are specced in the plan — change only with their tests.
- **Views**: Sidebar nav Library/Discover/Taste Profile (`DiscoverView.tsx` with For You + Wishlist tabs, `TasteProfileView.tsx`); GameDetail gets taste-fit + more-like-this.
- **Wishlist** (`src/wishlist.rs`): `IWishlistService/GetWishlist/v1` (works keyless for public profiles; returns appids only — names joined from catalog). 1h cache.
- **Model tiers** (`src/llm.rs` + `src/gpu.rs`): None / Qwen3.5-4B Q4 (2.7 GB) / Qwen3-8B Q4 (5 GB) / Qwen2.5-14B Q8 (15.7 GB "Max"). Active tier in `<app_data>/ai_settings.json`; legacy 14B installs auto-adopt (never delete other tiers' ggufs). VRAM probe: nvidia-smi → DXGI → RAM gate. llama-server stderr → `<app_data>/llama-server.log`. Chat candidates are taste-ranked (not Steam order); with no model the top taste picks ARE the recommendation (`used_llm: false`).
- **Store cache v2**: descriptions/metacritic/devs/release_date kept; versioned wrapper `{version: 2, details}` with legacy fallback; silent background backfill on cold start.
- Dev sanity tool: `cargo run --example taste_sanity` (runs the whole engine against the real local caches).

## Couch / TV play style (`src/couch.rs`)

Gamepad-friendliness for a TV setup, derived entirely from **store categories already cached** — no new network call, no new cache file, works offline.

- **Source data**: `StoreDetails::categories` (Steam `appdetails` category descriptions). Matching is **exact + case-insensitive**, never substring: "Tracked Controller Support" is a *VR* category and must not read as gamepad support.
- **Signals**: `Full controller support` / `Partial Controller Support` → `ControllerSupport`; `Shared/Split Screen{,Co-op,PvP}` → split screen; `Remote Play on TV`; `Remote Play Together`; `VR Only` (disqualifies — score forced negative). Empty or missing store entry → `Unknown`, never "no gamepad" (delisted games land here: ~20 of 571 in the dev library).
- **`qualifies(include_partial)`** is the single definition of "couch pick" and is mirrored in TS as `isCouchReady` in `commands.ts` — change both together.
- **Command**: `get_couch_profiles` returns a `{ appid: CouchProfile }` map built from current classifications + store cache.
- **Steam collection**: `write_to_steam` takes `WriteOptions { includeCouch, couchIncludePartial }` and appends **`Controller Friendly`** (excludes `NOT_A_GAME` and VR-only). Writing is **opt-in per write** (defaults on until the user unticks it) — opting out leaves any existing collection untouched rather than emptying it. `write_collection_sets` is the generic writer; `write_collections_to_steam` stays as the four-category wrapper. The command returns a `WriteReport { collections, removed }` so the UI reports what was actually written instead of restating the request.
- **UI**: sidebar "Play style" toggle (count reflects the current collection filter), the same toggle beside the grid search, controller badge on cards, "Couch & TV" block in `GameDetail`. Filter state lives in `App.tsx` (`CouchFilter`, persisted in `localStorage`) so sidebar and grid can never disagree.
- Dev sanity tool: `cargo run --example couch_sanity` (breakdown + collection preview from the real caches).

## Steam collection naming (no more `SBO:` prefix)

**Changed:** collections are written under plain names — `Completed`, `In Progress`, `Endless/Multiplayer`, `Not a Game`, `Controller Friendly`. "SBO" was the project's first name (Steam Backlog Organizer) and the prefix existed so the tool never touched hand-made collections. Users ended up with duplicate pairs, so Gamekeeper now **takes over collections with those names**.

- Existing collections with a matching name are **updated in place** (same collection id, Steam's `conflictResolutionMethod` / `strMethodId` fields preserved); their previous contents are replaced. Any other name is never touched.
- New collections are created with `conflictResolutionMethod: "custom"` + `strMethodId: "union-collections"`, matching what Steam itself stamps on user collections.
- `LEGACY_COLLECTION_NAMES` (the five `SBO:` names) are **deleted** on the next write, Steam-style: the entry's meta becomes `{key, timestamp, is_deleted: true, version}` with `value` dropped, and the key goes into `cloud-storage-namespace-1.modified.json` so the deletion syncs. A name in both `sets` and `remove` is written, never deleted.
- **Write modes** (`WriteOptions.mode`): `addNew` (**default**, non-destructive) keeps every current member and files a computed game only when no managed *category* collection already holds it — `merge_add_new` in `lib.rs`, with `claimed` being the union of the four category collections. The couch collection is orthogonal (a game can be In Progress *and* Controller Friendly), so it merges against itself alone. `replace` is the old overwrite and must stay opt-in; `WriteMode::default()` is `AddNew` and a test pins that, since a missing field must never silently mean "discard the user's sorting".
- The confirm dialog states that same-named collections get replaced — this writer is now destructive to user-named lists by design.

## Future feature ideas

1. **Delta catalog refresh** — `IStoreService/GetAppList?if_modified_since=` + appdetails/appreviews to update the catalog between releases (Phase 1.5; snapshot-per-release is fine for now).
2. **Multi-store (GOG/Epic)** — GOG Galaxy local SQLite + Epic manifests feeding the same classifier/taste pipeline (roadmap Phase 3).
3. **macOS/Linux / Steam Deck** — Steam paths and userdata differ by platform. Currently Windows-only (roadmap Phase 4).
4. **Streaming chat** — llama-server supports SSE; small/CPU tiers would feel much better with token streaming.

---

*End of CLAUDE.md — update this file when architecture or migration strategy changes.*
