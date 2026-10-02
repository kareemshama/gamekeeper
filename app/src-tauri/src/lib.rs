pub mod cache;
pub mod catalog;
pub mod classifier;
pub mod embed;
pub mod gpu;
pub mod collections;
pub mod config;
pub mod couch;
pub mod hltb;
pub mod llm;
pub mod steam_api;
pub mod taste;
pub mod wishlist;

use classifier::{Category, Classification};
use llm::LlmState;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

struct AppState {
    client: Client,
    games: Mutex<Vec<steam_api::OwnedGame>>,
    classifications: Mutex<Vec<Classification>>,
    store_cache: Mutex<HashMap<String, steam_api::StoreDetails>>,
    overrides: Mutex<HashMap<String, String>>,
    hltb_cache: Mutex<HashMap<String, hltb::HltbEntry>>,
    sync_cancelled: Arc<AtomicBool>,
    hltb_cancelled: Arc<AtomicBool>,
    hltb_fetching: Arc<AtomicBool>,
    store_backfill_running: Arc<AtomicBool>,
    // -- Taste engine (loaded in background at startup; None until ready) --
    catalog: Mutex<Option<Arc<catalog::Catalog>>>,
    embedder: Mutex<Option<Arc<embed::Embedder>>>,
    taste_profile: Mutex<Option<Arc<taste::TasteProfile>>>,
    taste_loading: Arc<AtomicBool>,
}

// -- Tauri commands --

#[derive(Serialize)]
struct ConfigStatus {
    configured: bool,
    steam_id: Option<String>,
}

#[tauri::command]
fn check_config() -> ConfigStatus {
    match config::load_config() {
        Ok(cfg) => ConfigStatus {
            configured: true,
            steam_id: Some(cfg.steam_id),
        },
        Err(_) => ConfigStatus {
            configured: false,
            steam_id: None,
        },
    }
}

#[tauri::command]
async fn save_config(state: State<'_, AppState>, api_key: String, steam_id: String) -> Result<(), String> {
    // Resolve vanity URL if the input isn't a numeric Steam ID
    let resolved_id = if steam_id.chars().all(|c| c.is_ascii_digit()) && steam_id.len() >= 15 {
        steam_id
    } else {
        // Treat as vanity URL name — need API key to resolve
        steam_api::resolve_vanity_url(&state.client, &api_key, &steam_id).await?
    };

    let cfg = config::AppConfig {
        steam_api_key: api_key,
        steam_id: resolved_id,
    };
    config::save_config(&cfg)
}

#[tauri::command]
async fn fetch_library(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<Vec<steam_api::OwnedGame>, String> {
    let cfg = config::load_config()?;
    let cancel = state.sync_cancelled.clone();
    cancel.store(false, Ordering::SeqCst);

    // Try cache first
    if let Some(cached) = cache::load_library_cache(&cfg.steam_id) {
        let mut games_lock = state.games.lock().map_err(|e| e.to_string())?;
        *games_lock = cached.clone();
        return Ok(cached);
    }

    // Fetch from Steam API (no mutex held across await)
    let mut games = steam_api::get_owned_games(&state.client, &cfg.steam_api_key, &cfg.steam_id)
        .await?;

    // Fetch achievements for all games
    steam_api::fetch_all_achievements(
        &state.client,
        &cfg.steam_api_key,
        &cfg.steam_id,
        &mut games,
        Some(&app),
        Some(&cancel),
    )
    .await;

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Cache the result
    let _ = cache::save_library_cache(&cfg.steam_id, &games);

    // Store in state (lock only briefly)
    {
        let mut state_games = state.games.lock().map_err(|e| e.to_string())?;
        *state_games = games.clone();
    }

    Ok(games)
}

#[tauri::command]
async fn fetch_store_details(state: State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let cancel = state.sync_cancelled.clone();

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Clone what we need out of the mutex, then drop the lock
    let app_ids: Vec<u64> = {
        let games = state.games.lock().map_err(|e| e.to_string())?;
        games.iter().map(|g| g.appid).collect()
    };
    let already_cached: std::collections::HashSet<String> = {
        let cache = state.store_cache.lock().map_err(|e| e.to_string())?;
        cache.keys().cloned().collect()
    };

    // Fetch without holding any locks
    let new_details =
        steam_api::fetch_store_details_batch(&state.client, &app_ids, &already_cached, Some(&app), Some(&cancel)).await?;

    if cancel.load(Ordering::SeqCst) {
        return Err("Sync cancelled".into());
    }

    // Merge results back into state
    {
        let mut store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
        store_cache.extend(new_details);
        cache::save_store_cache(&store_cache)?;
    }

    Ok(())
}

#[tauri::command]
fn cancel_sync(state: State<'_, AppState>) {
    state.sync_cancelled.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn classify_games(state: State<'_, AppState>) -> Result<Vec<Classification>, String> {
    let games = state.games.lock().map_err(|e| e.to_string())?;
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;

    // Load previously saved classifications to preserve stable results for unchanged games
    let saved = cache::load_saved_classifications();

    let results = classifier::classify_all_games(&games, &saved, &overrides, &store_cache);

    // Save to disk
    cache::save_classifications(&results)?;

    let mut classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    *classifications = results.clone();

    Ok(results)
}

#[tauri::command]
fn get_classifications(state: State<'_, AppState>) -> Result<Vec<Classification>, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    Ok(classifications.clone())
}

#[tauri::command]
fn set_override(
    state: State<'_, AppState>,
    appid: String,
    category: String,
) -> Result<(), String> {
    let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    overrides.insert(appid, category);
    cache::save_overrides(&overrides)
}

#[tauri::command]
fn remove_override(state: State<'_, AppState>, appid: String) -> Result<(), String> {
    let mut overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    overrides.remove(&appid);
    cache::save_overrides(&overrides)
}

#[tauri::command]
fn get_overrides(state: State<'_, AppState>) -> Result<HashMap<String, String>, String> {
    let overrides = state.overrides.lock().map_err(|e| e.to_string())?;
    Ok(overrides.clone())
}

#[derive(Serialize)]
struct CategorySummary {
    completed: usize,
    in_progress: usize,
    endless: usize,
    not_a_game: usize,
    total: usize,
}

#[tauri::command]
fn get_summary(state: State<'_, AppState>) -> Result<CategorySummary, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    let mut summary = CategorySummary {
        completed: 0,
        in_progress: 0,
        endless: 0,
        not_a_game: 0,
        total: classifications.len(),
    };
    for c in classifications.iter() {
        match c.category {
            Category::Completed => summary.completed += 1,
            Category::InProgress => summary.in_progress += 1,
            Category::Endless => summary.endless += 1,
            Category::NotAGame => summary.not_a_game += 1,
        }
    }
    Ok(summary)
}

/// Couch / TV play-style profiles for every classified game, keyed by app id.
///
/// Derived from the store details already on disk — instant, offline, and it
/// needs no extra cache file. Games whose store details were never fetched come
/// back as `unknown` rather than being omitted.
#[tauri::command]
fn get_couch_profiles(
    state: State<'_, AppState>,
) -> Result<HashMap<String, couch::CouchProfile>, String> {
    // Never hold both locks at once — `classify_games` takes them in the
    // opposite order, and two commands can run on different threads.
    let appids: Vec<u64> = {
        let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
        classifications.iter().map(|c| c.appid).collect()
    };
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
    Ok(couch::build_profiles(&appids, &store_cache))
}

#[tauri::command]
fn check_steam_running() -> bool {
    collections::is_steam_running()
}

#[tauri::command]
fn get_steam_accounts() -> Vec<collections::SteamAccount> {
    collections::get_steam_accounts()
}

/// How a write treats games already filed in the managed collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WriteMode {
    /// Never remove anything. A game is filed only when it isn't already in one
    /// of the four category collections, so hand-sorting survives untouched.
    #[default]
    AddNew,
    /// Each managed collection's contents become exactly what Gamekeeper
    /// computed. Discards any manual sorting in those collections.
    Replace,
}

/// Options for a Write to Steam run.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteOptions {
    /// Also write the "Controller Friendly" collection for a TV / couch setup.
    #[serde(default)]
    include_couch: bool,
    /// Let partial gamepad support into that collection.
    #[serde(default)]
    couch_include_partial: bool,
    /// Defaults to the non-destructive mode when the frontend omits it.
    #[serde(default)]
    mode: WriteMode,
}

/// Merge computed membership into what Steam already holds.
///
/// `existing` is the collection's current members, kept in order and never
/// dropped. A computed id joins it only when `claimed` doesn't already hold it —
/// `claimed` being every game filed across the managed category collections, so
/// a game the user sorted by hand is never duplicated into a second collection.
fn merge_add_new(existing: &[u64], computed: &[u64], claimed: &HashSet<u64>) -> Vec<u64> {
    let mut out = existing.to_vec();
    let mut seen: HashSet<u64> = existing.iter().copied().collect();
    for id in computed {
        if claimed.contains(id) || seen.contains(id) {
            continue;
        }
        seen.insert(*id);
        out.push(*id);
    }
    out
}

/// What a write actually put into Steam — reported back so the UI can state
/// facts instead of repeating what it asked for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WriteReport {
    /// `(collection name, game count)` in the order written.
    collections: Vec<(String, usize)>,
    /// Old `SBO:`-prefixed collections deleted during this write.
    removed: Vec<String>,
}

#[tauri::command]
fn write_to_steam(
    state: State<'_, AppState>,
    account_path: String,
    options: Option<WriteOptions>,
) -> Result<WriteReport, String> {
    // Check if Steam is running
    if collections::is_steam_running() {
        return Err("Steam is currently running. Please close Steam before writing collections.".into());
    }

    let options = options.unwrap_or_default();

    // Load existing cloud data
    let userdata_path = std::path::PathBuf::from(&account_path);
    let (mut cloud_data, cloud_path) = collections::load_steam_collections(&userdata_path)?;

    // Build categories from current classifications. Snapshot the (appid,
    // category) pairs and release the lock before touching the store cache —
    // `classify_games` locks these two in the opposite order.
    let games: Vec<(u64, Category)> = {
        let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
        classifications
            .iter()
            .map(|c| (c.appid, c.category.clone()))
            .collect()
    };

    let mut categories: HashMap<String, Vec<u64>> = HashMap::new();
    for key in collections::CATEGORY_ORDER {
        categories.insert(key.to_string(), Vec::new());
    }

    for (appid, category) in &games {
        categories
            .entry(category.to_string())
            .or_default()
            .push(*appid);
    }

    let coll_names = collections::collection_names();
    let mut sets: Vec<(String, Vec<u64>)> = collections::CATEGORY_ORDER
        .iter()
        .map(|key| {
            (
                coll_names[key].to_string(),
                categories.get(*key).cloned().unwrap_or_default(),
            )
        })
        .collect();

    if options.include_couch {
        let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?;
        let couch_ids: Vec<u64> = games
            .iter()
            // Soundtracks, tools and demos are noise on a TV.
            .filter(|(_, category)| *category != Category::NotAGame)
            .filter(|(appid, _)| {
                couch::profile_for(*appid, store_cache.get(&appid.to_string()))
                    .qualifies(options.couch_include_partial)
            })
            .map(|(appid, _)| *appid)
            .collect();
        sets.push((collections::COUCH_COLLECTION_NAME.to_string(), couch_ids));
    }

    // "Add new games only": keep every current member, and file a computed game
    // only when no managed category collection already holds it. The couch
    // collection is a play style, orthogonal to the four categories, so it is
    // merged against itself alone — a game can be both In Progress and
    // Controller Friendly.
    if options.mode == WriteMode::AddNew {
        let members = collections::existing_collection_members(&cloud_data);
        let category_names: HashSet<&str> = collections::CATEGORY_ORDER
            .iter()
            .map(|key| coll_names[key])
            .collect();
        let claimed: HashSet<u64> = category_names
            .iter()
            .filter_map(|name| members.get(*name))
            .flatten()
            .copied()
            .collect();

        for (name, computed) in sets.iter_mut() {
            let existing = members.get(name).map(|v| v.as_slice()).unwrap_or(&[]);
            *computed = if category_names.contains(name.as_str()) {
                merge_add_new(existing, computed, &claimed)
            } else {
                merge_add_new(existing, computed, &HashSet::new())
            };
        }
    }

    eprintln!(
        "[collections] mode {:?}, writing {} collections (include_couch = {}): {}",
        options.mode,
        sets.len(),
        options.include_couch,
        sets.iter()
            .map(|(name, ids)| format!("{name} ({})", ids.len()))
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Write, clearing the old SBO:-prefixed collections in the same pass
    let removed = collections::write_collection_sets(
        &mut cloud_data,
        &cloud_path,
        &sets,
        &collections::LEGACY_COLLECTION_NAMES,
    )?;
    if !removed.is_empty() {
        eprintln!("[collections] removed legacy collections: {}", removed.join(", "));
    }

    Ok(WriteReport {
        collections: sets
            .into_iter()
            .map(|(name, ids)| (name, ids.len()))
            .collect(),
        removed,
    })
}

// -- AI / LLM commands (bundled llama-server) --

#[tauri::command]
async fn check_ai_setup(app: tauri::AppHandle) -> Result<llm::SetupStatus, String> {
    let data_dir = llm::get_data_dir(&app);
    Ok(llm::check_setup(&data_dir))
}

/// Download the llama-server binary plus the given tier's model (defaults to
/// the hardware-recommended tier, then the smallest).
#[tauri::command]
async fn setup_ai(app: tauri::AppHandle, tier: Option<String>) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    let status = llm::check_setup(&data_dir);

    let tier = match tier.as_deref() {
        Some(id) => llm::tier_by_id(id).ok_or_else(|| format!("unknown tier {id}"))?,
        None => {
            let hw = gpu::probe();
            hw.recommended_tier
                .as_deref()
                .and_then(llm::tier_by_id)
                .unwrap_or(&llm::MODEL_TIERS[0])
        }
    };

    if !status.server_ready {
        llm::download_server(&data_dir, &app).await?;
    }
    llm::download_model(&data_dir, &app, tier).await?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelTierInfo {
    id: String,
    label: String,
    size_bytes: u64,
    installed: bool,
    active: bool,
    recommended: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelTiersResponse {
    tiers: Vec<ModelTierInfo>,
    vram_mb: Option<u64>,
    ram_mb: u64,
}

#[tauri::command]
fn get_model_tiers(app: tauri::AppHandle) -> ModelTiersResponse {
    let data_dir = llm::get_data_dir(&app);
    let hw = gpu::probe();
    let installed = llm::installed_tier_ids(&data_dir);
    let active = llm::active_tier(&data_dir).map(|t| t.id);
    ModelTiersResponse {
        tiers: llm::MODEL_TIERS
            .iter()
            .map(|t| ModelTierInfo {
                id: t.id.into(),
                label: t.label.into(),
                size_bytes: t.size_bytes,
                installed: installed.contains(&t.id),
                active: active == Some(t.id),
                recommended: hw.recommended_tier.as_deref() == Some(t.id),
            })
            .collect(),
        vram_mb: hw.vram_mb,
        ram_mb: hw.ram_mb,
    }
}

/// Switch the active tier (must already be installed). Restarts the server.
#[tauri::command]
fn set_model_tier(
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
    tier: String,
) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    let t = llm::tier_by_id(&tier).ok_or_else(|| format!("unknown tier {tier}"))?;
    if !llm::get_model_path_for(&data_dir, t).exists() {
        return Err("Model not downloaded yet".into());
    }
    llm::stop_server(&state);
    llm::set_active_tier(&data_dir, Some(t.id));
    Ok(())
}

/// Delete one tier's model file to free disk space.
#[tauri::command]
fn delete_model_tier(
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
    tier: String,
) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    let t = llm::tier_by_id(&tier).ok_or_else(|| format!("unknown tier {tier}"))?;
    if llm::active_tier(&data_dir).map(|a| a.id) == Some(t.id) {
        llm::stop_server(&state);
    }
    llm::delete_model(&data_dir, t)
}

#[tauri::command]
fn get_gpu_status(
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
) -> llm::GpuStatus {
    let data_dir = llm::get_data_dir(&app);
    llm::get_gpu_status(&data_dir, &state)
}

#[tauri::command]
fn set_gpu_enabled(
    enabled: bool,
    app: tauri::AppHandle,
    state: State<'_, LlmState>,
) -> Result<(), String> {
    let data_dir = llm::get_data_dir(&app);
    llm::set_force_cpu(&state, !enabled, &data_dir);
    // Restart server with new setting
    llm::stop_server(&state);
    llm::start_server(&data_dir, &state)
}

#[derive(Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct RecommendRequest {
    message: String,
    #[serde(default)]
    history: Vec<ChatMessage>,
}

#[derive(Serialize)]
struct RecommendResponse {
    picks: Vec<llm::GameRecommendation>,
    used_llm: bool,
    message: String,
}

/// Strip any JSON that the LLM leaked into its message text.
/// The model sometimes prepends or appends raw JSON syntax around conversational text.
fn clean_llm_message(message: &str) -> String {
    // Patterns that indicate leaked JSON
    let json_patterns = [
        " - {\"", " - [{", " - \"picks\"", " - \"message\"", " - []",
        "[{\"", "{\"message\"", "{\"picks\"",
        "\"picks\":", "\"message\":",
    ];

    let mut result = message.to_string();

    // Step 1: Strip JSON from the beginning (e.g. '{"picks": []} actual message here')
    // Try to find where a JSON object/array at the start ends
    if result.starts_with('{') || result.starts_with('[') {
        // Find the end of the leading JSON structure
        let mut depth = 0;
        let mut in_string = false;
        let mut escape_next = false;
        let mut end_idx = 0;
        for (i, ch) in result.char_indices() {
            if escape_next {
                escape_next = false;
                continue;
            }
            if ch == '\\' && in_string {
                escape_next = true;
                continue;
            }
            if ch == '"' {
                in_string = !in_string;
                continue;
            }
            if in_string {
                continue;
            }
            if ch == '{' || ch == '[' {
                depth += 1;
            } else if ch == '}' || ch == ']' {
                depth -= 1;
                if depth == 0 {
                    end_idx = i + 1;
                    break;
                }
            }
        }
        if end_idx > 0 && end_idx < result.len() {
            // There's text after the JSON — use that as the message
            result = result[end_idx..].trim().to_string();
        } else if end_idx > 0 {
            // The entire message is JSON — try to extract "message" field from it
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&result[..end_idx]) {
                if let Some(msg) = parsed.get("message").and_then(|m| m.as_str()) {
                    result = msg.to_string();
                }
            }
        }
    }

    // Step 2: Strip JSON from the end
    let mut earliest_cut = result.len();
    for pattern in &json_patterns {
        if let Some(idx) = result.find(pattern) {
            earliest_cut = earliest_cut.min(idx);
        }
    }
    let cleaned = result[..earliest_cut]
        .trim_end_matches(|c: char| c == '-' || c == ' ' || c == '\n' || c == ',');

    let final_msg = if cleaned.is_empty() {
        result.trim().to_string()
    } else {
        cleaned.to_string()
    };

    // Strip markdown formatting (bold **text** → text, *text* → text)
    let mut out = final_msg;
    while out.contains("**") {
        out = out.replacen("**", "", 2);
    }
    while out.contains("__") {
        out = out.replacen("__", "", 2);
    }
    out
}

/// Validate picks against the real game library — fix appids/titles, drop unverifiable picks.
fn validate_picks(
    raw_picks: Vec<llm::GameRecommendation>,
    classifications: &[Classification],
) -> Vec<llm::GameRecommendation> {
    raw_picks
        .into_iter()
        .filter_map(|mut pick| {
            if let Some(real_game) = classifications.iter().find(|c| c.appid == pick.appid) {
                pick.title = real_game.name.clone();
                return Some(pick);
            }
            let pick_title_lower = pick.title.to_lowercase();
            if let Some(real_game) = classifications.iter().find(|c| {
                c.name.to_lowercase() == pick_title_lower
            }) {
                pick.appid = real_game.appid;
                pick.title = real_game.name.clone();
                return Some(pick);
            }
            if pick_title_lower.len() >= 8 {
                if let Some(real_game) = classifications.iter().find(|c| {
                    let name_lower = c.name.to_lowercase();
                    name_lower.starts_with(&pick_title_lower)
                        || pick_title_lower.starts_with(&name_lower)
                }) {
                    pick.appid = real_game.appid;
                    pick.title = real_game.name.clone();
                    return Some(pick);
                }
            }
            None
        })
        .collect()
}

/// Robustly parse an LLM response that may be clean JSON, malformed JSON, or
/// a mix of conversational text and JSON. Returns (message, picks).
fn parse_llm_response(response: &str) -> (String, Vec<llm::GameRecommendation>) {
    // Strategy 1: Try clean JSON parse
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(response) {
        let raw_message = parsed
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let picks = parsed
            .get("picks")
            .and_then(|p| p.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| serde_json::from_value::<llm::GameRecommendation>(p.clone()).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // If we got a clean parse with picks, great
        if !picks.is_empty() {
            return (clean_llm_message(&raw_message), picks);
        }

        // Clean parse but no picks — try salvaging from message text
        let message = clean_llm_message(&raw_message);
        if let Some(salvaged) = extract_picks_from_message(&raw_message) {
            return (message, salvaged);
        }
        return (message, Vec::new());
    }

    // Strategy 2: JSON parse failed — try to find a JSON object anywhere in the text
    // The model sometimes outputs: "conversational text {"message": "...", "picks": [...]}"
    if let Some(obj_start) = response.find("{\"") {
        let json_candidate = &response[obj_start..];
        // Try to fix common JSON errors (trailing commas, double braces)
        let fixed = fix_json(json_candidate);
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&fixed) {
            let inner_message = parsed
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let picks = parsed
                .get("picks")
                .and_then(|p| p.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| serde_json::from_value::<llm::GameRecommendation>(p.clone()).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            // Use the text before the JSON as the message if inner message is empty
            let pre_json = clean_llm_message(&response[..obj_start]);
            let message = if !inner_message.is_empty() {
                clean_llm_message(&inner_message)
            } else if !pre_json.is_empty() {
                pre_json
            } else {
                String::new()
            };
            return (message, picks);
        }
    }

    // Strategy 3: Try to extract picks from any JSON array in the text
    let message = clean_llm_message(response);
    let picks = extract_picks_from_message(response).unwrap_or_default();
    (message, picks)
}

/// Attempt to fix common JSON errors from the LLM (e.g. double braces, trailing commas).
fn fix_json(input: &str) -> String {
    let mut s = input.to_string();
    // Fix double closing braces: }}} → }}
    while s.contains("}}}") {
        s = s.replace("}}}", "}}");
    }
    // Fix trailing commas before closing brackets/braces
    loop {
        let new = s.replace(",]", "]").replace(",}", "}");
        if new == s {
            break;
        }
        s = new;
    }
    s
}

/// Try to extract game picks from JSON embedded in the LLM's message text.
/// Handles multiple formats the model might produce:
/// 1. Raw array: [{"appid": 123, ...}]
/// 2. Nested object: {"message": "...", "picks": [...]}
fn extract_picks_from_message(message: &str) -> Option<Vec<llm::GameRecommendation>> {
    // Try to find a nested JSON object with a "picks" field
    if let Some(obj_start) = message.find("{\"message\"").or_else(|| message.find("{\"picks\"")) {
        let json_candidate = &message[obj_start..];
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_candidate) {
            if let Some(picks_arr) = parsed.get("picks").and_then(|p| p.as_array()) {
                let picks: Vec<llm::GameRecommendation> = picks_arr
                    .iter()
                    .filter_map(|p| serde_json::from_value(p.clone()).ok())
                    .collect();
                if !picks.is_empty() {
                    return Some(picks);
                }
            }
        }
    }

    // Try to find a raw JSON array of picks
    let start = message.find("[{")?;
    let end = message.rfind("}]").map(|i| i + 2)?;
    if end <= start {
        return None;
    }
    let json_str = &message[start..end];
    let picks: Vec<llm::GameRecommendation> = serde_json::from_str(json_str).ok()?;
    if picks.is_empty() {
        return None;
    }
    Some(picks)
}

/// Build candidate summaries for AI inference, enriched with HLTB data.
fn build_candidate_summaries(
    candidates: &[&Classification],
    games: &[steam_api::OwnedGame],
    store_cache: &HashMap<String, steam_api::StoreDetails>,
    hltb_cache: &HashMap<String, hltb::HltbEntry>,
    max_candidates: usize,
) -> Vec<serde_json::Value> {
    let mut summaries = Vec::new();
    for c in candidates.iter().take(max_candidates) {
        let playtime = games
            .iter()
            .find(|g| g.appid == c.appid)
            .map(|g| g.playtime_hours)
            .unwrap_or(0.0);

        let store = store_cache.get(&c.appid.to_string());
        let genres = store.map(|s| s.genres.join(", ")).unwrap_or_default();
        let categories = store.map(|s| s.categories.join(", ")).unwrap_or_default();

        let hltb = hltb_cache.get(&c.appid.to_string());
        let main_story_hours = hltb.and_then(|h| h.main_story_hours);
        let time_remaining = main_story_hours.map(|msh| ((msh - playtime).max(0.0) * 10.0).round() / 10.0);

        let mut summary = serde_json::json!({
            "appid": c.appid,
            "title": c.name,
            "category": c.category.to_string(),
            "playtime_hours": playtime,
            "genres": genres,
            "store_tags": categories,
        });

        // Include HLTB data compactly when available
        if let Some(hours) = main_story_hours {
            summary["hltb_hours"] = serde_json::json!(hours);
        }
        if let Some(remaining) = time_remaining {
            summary["hours_left"] = serde_json::json!(remaining);
        }

        summaries.push(summary);
    }
    summaries
}

#[tauri::command]
async fn get_recommendations(
    state: State<'_, AppState>,
    llm_state: State<'_, LlmState>,
    app: tauri::AppHandle,
    request: RecommendRequest,
) -> Result<RecommendResponse, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?.clone();
    let games = state.games.lock().map_err(|e| e.to_string())?.clone();
    let hltb_cache = state.hltb_cache.lock().map_err(|e| e.to_string())?.clone();

    // Step 1: Deterministic candidate filtering
    let mut candidates: Vec<&Classification> = classifications
        .iter()
        .filter(|c| c.category == Category::InProgress || c.category == Category::Endless)
        .collect();

    if candidates.is_empty() {
        return Ok(RecommendResponse {
            picks: Vec::new(),
            used_llm: false,
            message: "You don't have any games in your backlog or endless categories yet.".into(),
        });
    }

    // Step 1b: Taste-ranked retrieval — order candidates by taste match blended
    // with the message's own semantics, instead of Steam API order. Games
    // missing from the catalog keep their relative order at the end.
    let taste_ctx: Option<(Arc<catalog::Catalog>, Arc<taste::TasteProfile>)> = {
        let cat = state.catalog.lock().map_err(|e| e.to_string())?.clone();
        let profile = state.taste_profile.lock().map_err(|e| e.to_string())?.clone();
        cat.zip(profile)
    };
    if let Some((cat, profile)) = &taste_ctx {
        let query_vec = state
            .embedder
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .filter(|_| !request.message.trim().is_empty())
            .map(|em| em.embed(&request.message));
        let mut ranked: Vec<(f32, usize)> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let score = match cat.get(c.appid as u32) {
                    Some((row, _)) => {
                        let v = cat.vector_f32(row);
                        let taste_sim = taste::dot(&profile.vector, &v);
                        match &query_vec {
                            Some(q) => 0.7 * taste_sim + 0.3 * taste::dot(q, &v),
                            None => taste_sim,
                        }
                    }
                    None => f32::NEG_INFINITY, // sort to the end, stable
                };
                (score, i)
            })
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        candidates = ranked.iter().map(|(_, i)| candidates[*i]).collect();
    }

    // Fewer candidates on follow-ups to reduce prompt size and speed up inference
    let max_candidates = if request.history.is_empty() { 40 } else { 15 };
    let candidate_summaries = build_candidate_summaries(&candidates, &games, &store_cache, &hltb_cache, max_candidates);

    // Step 2: Check if AI is set up
    let data_dir = llm::get_data_dir(&app);
    let setup = llm::check_setup(&data_dir);
    let ai_available = setup.model_ready && setup.server_ready;

    if ai_available {
        // AI is downloaded — always use it, never fall back to rules
        llm::start_server(&data_dir, &llm_state)
            .map_err(|e| format!("Failed to start AI engine: {e}"))?;
        llm::wait_for_server().await
            .map_err(|e| format!("AI engine not responding: {e}"))?;

        let candidates_json =
            serde_json::to_string_pretty(&candidate_summaries).unwrap_or_default();

        // Convert history to LLM chat messages (limit to last 6 messages
        // to keep context manageable for the local model)
        let history_start = request.history.len().saturating_sub(6);
        let chat_history: Vec<(String, String)> = request
            .history[history_start..]
            .iter()
            .map(|m| (m.role.clone(), m.content.clone()))
            .collect();

        let response_text = llm::run_recommendation_inference(
            &candidates_json,
            &request.message,
            &chat_history,
        ).await?;

        // Parse the LLM response — handles clean JSON and various malformed outputs
        let (message, raw_picks) = parse_llm_response(&response_text);
        let validated_picks = validate_picks(raw_picks, &classifications);

        return Ok(RecommendResponse {
            picks: validated_picks,
            used_llm: true,
            message,
        });
    }

    // Step 3: No model installed — candidates are already taste-ranked above,
    // so the top picks ARE the recommendation. Deterministic reasons.
    let fallback: Vec<llm::GameRecommendation> = candidates
        .iter()
        .take(3)
        .map(|c| {
            let playtime = games
                .iter()
                .find(|g| g.appid == c.appid)
                .map(|g| g.playtime_hours)
                .unwrap_or(0.0);
            let reason = match &taste_ctx {
                Some((cat, profile)) => match cat.get(c.appid as u32) {
                    Some((row, meta)) => {
                        let v = cat.vector_f32(row);
                        taste::reason_for(profile, &v, &meta.tags)
                    }
                    None if playtime == 0.0 => "Unplayed — waiting in your backlog".into(),
                    None => format!("Only {playtime:.1}h played — might be worth revisiting"),
                },
                None if playtime == 0.0 => "Unplayed — waiting in your backlog".into(),
                None => format!("Only {playtime:.1}h played — might be worth revisiting"),
            };
            llm::GameRecommendation {
                appid: c.appid,
                title: c.name.clone(),
                reason,
            }
        })
        .collect();

    let message = if taste_ctx.is_some() {
        "Picked from your backlog by taste match:"
    } else {
        "Here are some suggestions from your backlog:"
    };
    Ok(RecommendResponse {
        picks: fallback,
        used_llm: false,
        message: message.into(),
    })
}

#[derive(Deserialize)]
struct AmbiguityRequest {
    appid: u64,
}

#[derive(Serialize)]
struct AmbiguityResponse {
    suggested_category: String,
    rationale: String,
    used_llm: bool,
}

#[tauri::command]
async fn get_ambiguity_suggestion(
    state: State<'_, AppState>,
    llm_state: State<'_, LlmState>,
    app: tauri::AppHandle,
    request: AmbiguityRequest,
) -> Result<AmbiguityResponse, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?.clone();
    let games = state.games.lock().map_err(|e| e.to_string())?.clone();

    let classification = classifications
        .iter()
        .find(|c| c.appid == request.appid)
        .ok_or("Game not found in classifications")?;

    let game = games
        .iter()
        .find(|g| g.appid == request.appid);

    let store_info = store_cache.get(&request.appid.to_string());

    // Check if AI is set up
    let data_dir = llm::get_data_dir(&app);
    let setup = llm::check_setup(&data_dir);
    if !setup.model_ready || !setup.server_ready {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: format!("AI not set up. Current: {}", classification.reason),
            used_llm: false,
        });
    }

    // Try to start server and run inference
    if let Err(_) = llm::start_server(&data_dir, &llm_state) {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: "Failed to start AI engine. Using rule-based classification.".into(),
            used_llm: false,
        });
    }

    if let Err(_) = llm::wait_for_server().await {
        return Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: "AI engine not ready. Using rule-based classification.".into(),
            used_llm: false,
        });
    }

    let playtime = game.map(|g| g.playtime_hours).unwrap_or(0.0);
    let genres = store_info
        .map(|s| s.genres.join(", "))
        .unwrap_or_else(|| "Unknown".into());
    let categories = store_info
        .map(|s| s.categories.join(", "))
        .unwrap_or_else(|| "Unknown".into());

    match llm::run_ambiguity_inference(
        &classification.name,
        &genres,
        &categories,
        playtime,
        &classification.category.to_string(),
        &classification.reason,
    )
    .await
    {
        Ok(response_text) => {
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&response_text) {
                let cat = parsed
                    .get("suggested_category")
                    .and_then(|c| c.as_str())
                    .unwrap_or("IN_PROGRESS")
                    .to_string();
                let rationale = parsed
                    .get("rationale")
                    .and_then(|r| r.as_str())
                    .unwrap_or("No rationale provided")
                    .to_string();
                return Ok(AmbiguityResponse {
                    suggested_category: cat,
                    rationale,
                    used_llm: true,
                });
            }
            Ok(AmbiguityResponse {
                suggested_category: classification.category.to_string(),
                rationale: "Failed to parse AI response. Using rule-based.".into(),
                used_llm: false,
            })
        }
        Err(e) => Ok(AmbiguityResponse {
            suggested_category: classification.category.to_string(),
            rationale: format!("AI error: {e}. Using rule-based."),
            used_llm: false,
        }),
    }
}

// -- HLTB --

#[tauri::command]
fn get_hltb_cache(_state: State<'_, AppState>) -> Result<HashMap<String, hltb::HltbEntry>, String> {
    // Read from disk to always get the latest data, even during background fetch
    Ok(cache::load_hltb_cache())
}

#[tauri::command]
async fn fetch_hltb_data(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Prevent concurrent HLTB fetches
    if state.hltb_fetching.load(Ordering::SeqCst) {
        return Ok(());
    }

    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let existing_cache = state.hltb_cache.lock().map_err(|e| e.to_string())?.clone();
    let cancel = state.hltb_cancelled.clone();
    let fetching = state.hltb_fetching.clone();

    // Reset cancel flag for new fetch
    cancel.store(false, Ordering::SeqCst);
    fetching.store(true, Ordering::SeqCst);

    // Build game list with categories for priority sorting
    let games: Vec<(u64, String, String)> = classifications
        .iter()
        .map(|c| (c.appid, c.name.clone(), c.category.to_string()))
        .collect();

    // Spawn background task — updates are saved to disk by fetch_hltb_batch.
    // Frontend re-reads via get_hltb_cache after hltb-complete event.
    let app_handle = app.clone();
    tokio::spawn(async move {
        match hltb::fetch_hltb_batch(games, existing_cache, app_handle.clone(), cancel).await {
            Ok(new_cache) => {
                let app_state: tauri::State<AppState> = app_handle.state();
                match app_state.hltb_cache.lock() {
                    Ok(mut guard) => { *guard = new_cache; }
                    Err(e) => eprintln!("[HLTB] Failed to update cache: {e}"),
                };
            }
            Err(e) => {
                eprintln!("HLTB fetch error: {e}");
            }
        }
        fetching.store(false, Ordering::SeqCst);
    });

    Ok(())
}

/// Return the cached library without any freshness check or network fallback.
/// Used to hydrate playtime display on cold start; empty vec when no cache.
#[tauri::command]
async fn get_cached_library(state: State<'_, AppState>) -> Result<Vec<steam_api::OwnedGame>, String> {
    let cfg = config::load_config()?;
    let games = cache::load_library_cache_any_age(&cfg.steam_id).unwrap_or_default();
    if !games.is_empty() {
        let mut games_lock = state.games.lock().map_err(|e| e.to_string())?;
        if games_lock.is_empty() {
            *games_lock = games.clone();
        }
    }
    Ok(games)
}

/// Backfill v2 store fields (short_description etc.) for entries cached before
/// the taste engine existed. Silent background task; self-checks and no-ops when
/// nothing needs backfilling. Rate-limited like the normal store sync.
#[tauri::command]
async fn backfill_store_details(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if state.store_backfill_running.swap(true, Ordering::SeqCst) {
        return Ok(()); // already running
    }

    let to_fetch = match state.store_cache.lock() {
        Ok(store_cache) => cache::store_entries_needing_backfill(&store_cache),
        Err(e) => {
            state.store_backfill_running.store(false, Ordering::SeqCst);
            return Err(e.to_string());
        }
    };
    if to_fetch.is_empty() {
        state.store_backfill_running.store(false, Ordering::SeqCst);
        return Ok(());
    }

    let client = state.client.clone();
    let running = state.store_backfill_running.clone();
    let app_handle = app.clone();
    tokio::spawn(async move {
        // Empty "already cached" set: these appids ARE cached, but as v1 entries
        // we deliberately re-fetch. No progress handle — this runs silently.
        let fetched = steam_api::fetch_store_details_batch(
            &client,
            &to_fetch,
            &std::collections::HashSet::new(),
            None,
            None,
        )
        .await;

        if let Ok(new_details) = fetched {
            let updated = {
                let app_state: tauri::State<AppState> = app_handle.state();
                let snapshot = match app_state.store_cache.lock() {
                    Ok(mut guard) => {
                        guard.extend(new_details);
                        Some(guard.clone())
                    }
                    Err(e) => {
                        eprintln!("[Backfill] Failed to update store cache: {e}");
                        None
                    }
                };
                snapshot
            };
            if let Some(cache_snapshot) = updated {
                if let Err(e) = cache::save_store_cache(&cache_snapshot) {
                    eprintln!("[Backfill] Failed to save store cache: {e}");
                }
                use tauri::Emitter;
                let _ = app_handle.emit("store-backfill-complete", cache_snapshot.len());
            }
        }
        running.store(false, Ordering::SeqCst);
    });

    Ok(())
}

// -- Taste engine --

/// Resolve the catalog file: an updated copy in app data wins over the bundled
/// resource (future catalog-update path).
fn catalog_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let updated = llm::get_data_dir(app).join("catalog").join("catalog.gkc");
    if updated.exists() {
        return Some(updated);
    }
    app.path()
        .resolve("resources/catalog.gkc", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.exists())
}

fn embed_model_dir(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path()
        .resolve("resources/potion-base-8M", tauri::path::BaseDirectory::Resource)
        .ok()
        .filter(|p| p.join("model.safetensors").exists())
}

/// Load catalog + embedder off the main thread; emits "taste-ready" when done.
fn load_taste_assets_background(app: tauri::AppHandle) {
    let state: State<'_, AppState> = app.state();
    if state.taste_loading.swap(true, Ordering::SeqCst) {
        return;
    }
    let loading = state.taste_loading.clone();
    let app_handle = app.clone();
    std::thread::spawn(move || {
        // catch_unwind so a panic (e.g. from a pathological catalog file) can
        // never leave taste_loading stuck at true with no taste-ready event.
        let handle = app_handle.clone();
        let catalog_ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            load_taste_assets(&handle)
        }))
        .unwrap_or_else(|_| {
            eprintln!("[Taste] Asset loading panicked");
            false
        });
        loading.store(false, Ordering::SeqCst);
        use tauri::Emitter;
        let _ = app_handle.emit("taste-ready", catalog_ok);
    });
}

/// Load catalog + embedder into state; returns whether the catalog loaded.
fn load_taste_assets(app_handle: &tauri::AppHandle) -> bool {
    {
        let mut catalog_ok = false;
        if let Some(path) = catalog_path(app_handle) {
            match catalog::Catalog::load(&path) {
                Ok(cat) => {
                    let state: State<'_, AppState> = app_handle.state();
                    match state.catalog.lock() {
                        Ok(mut guard) => {
                            *guard = Some(Arc::new(cat));
                            catalog_ok = true;
                        }
                        Err(e) => eprintln!("[Taste] Catalog mutex poisoned: {e}"),
                    };
                }
                Err(e) => eprintln!("[Taste] Catalog load failed: {e}"),
            }
        }
        if let Some(dir) = embed_model_dir(app_handle) {
            match embed::Embedder::load(&dir) {
                Ok(em) => {
                    let state: State<'_, AppState> = app_handle.state();
                    match state.embedder.lock() {
                        Ok(mut guard) => *guard = Some(Arc::new(em)),
                        Err(e) => eprintln!("[Taste] Embedder mutex poisoned: {e}"),
                    };
                }
                Err(e) => eprintln!("[Taste] Embedder load failed: {e}"),
            }
        }
        catalog_ok
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TasteSetupStatus {
    catalog_installed: bool,
    /// yyyymmdd of the catalog's source dataset, e.g. 20260901.
    catalog_dataset_date: Option<u32>,
    catalog_game_count: Option<u32>,
    embed_model_installed: bool,
    loading: bool,
}

#[tauri::command]
fn check_taste_setup(state: State<'_, AppState>) -> Result<TasteSetupStatus, String> {
    let catalog = state.catalog.lock().map_err(|e| e.to_string())?;
    let embedder = state.embedder.lock().map_err(|e| e.to_string())?;
    Ok(TasteSetupStatus {
        catalog_installed: catalog.is_some(),
        catalog_dataset_date: catalog.as_ref().map(|c| c.header.dataset_date),
        catalog_game_count: catalog.as_ref().map(|c| c.header.game_count),
        embed_model_installed: embedder.is_some(),
        loading: state.taste_loading.load(Ordering::SeqCst),
    })
}

/// Assemble per-game signals from everything cached locally.
pub fn build_game_signals(
    games: &[steam_api::OwnedGame],
    classifications: &[Classification],
    store_cache: &HashMap<String, steam_api::StoreDetails>,
    hltb_cache: &HashMap<String, hltb::HltbEntry>,
    cat: &catalog::Catalog,
    embedder: Option<&embed::Embedder>,
) -> Vec<taste::GameSignal> {
    let class_by_id: HashMap<u64, &Classification> =
        classifications.iter().map(|c| (c.appid, c)).collect();

    games
        .iter()
        .map(|g| {
            let category = class_by_id
                .get(&g.appid)
                .map(|c| c.category.clone())
                .unwrap_or(Category::InProgress);
            let appid_str = g.appid.to_string();
            let hltb_main = hltb_cache
                .get(&appid_str)
                .and_then(|h| h.main_story_hours);

            let (vector, tags) = match cat.get(g.appid as u32) {
                Some((row, meta)) => (Some(cat.vector_f32(row)), meta.tags.clone()),
                None => {
                    // Not in catalog: runtime-embed from cached store text (same space)
                    let fallback = embedder.and_then(|em| {
                        store_cache.get(&appid_str).and_then(|d| {
                            d.short_description.as_ref().map(|desc| {
                                em.embed(&embed::compose_embed_text(&[], &d.genres, desc))
                            })
                        })
                    });
                    (fallback, Vec::new())
                }
            };

            taste::GameSignal {
                appid: g.appid,
                name: g.name.clone(),
                hours: g.playtime_hours,
                hours_2weeks: g.playtime_2weeks_hours,
                rtime_last_played: g.rtime_last_played,
                ach_pct: g.achievements.as_ref().map(|a| a.percentage),
                category,
                hltb_main_hours: hltb_main,
                vector,
                tags,
            }
        })
        .collect()
}

/// Compute (or return cached) taste profile. Recomputes when `force` is true.
/// Async + spawn_blocking: runtime-embedding catalog-missing games and the
/// profile math must never run on the main (webview) thread.
#[tauri::command]
async fn get_taste_profile(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<taste::TasteProfile, String> {
    if !force.unwrap_or(false) {
        if let Some(profile) = state.taste_profile.lock().map_err(|e| e.to_string())?.as_ref() {
            return Ok(profile.as_ref().clone());
        }
    }

    let cat = state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("CATALOG_NOT_READY")?;
    let embedder = state.embedder.lock().map_err(|e| e.to_string())?.clone();
    let games = state.games.lock().map_err(|e| e.to_string())?.clone();
    if games.is_empty() {
        return Err("LIBRARY_NOT_LOADED".into());
    }
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?.clone();
    let store_cache = state.store_cache.lock().map_err(|e| e.to_string())?.clone();
    let hltb_cache = state.hltb_cache.lock().map_err(|e| e.to_string())?.clone();

    let profile = tauri::async_runtime::spawn_blocking(move || {
        let signals = build_game_signals(
            &games,
            &classifications,
            &store_cache,
            &hltb_cache,
            &cat,
            embedder.as_deref(),
        );
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        taste::compute_profile(&signals, now)
    })
    .await
    .map_err(|e| format!("profile compute failed: {e}"))?;

    *state.taste_profile.lock().map_err(|e| e.to_string())? = Some(Arc::new(profile.clone()));
    Ok(profile)
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct DiscoverFilters {
    min_review_pct: Option<u8>,
    min_reviews: Option<u32>,
    require_price: Option<bool>,
    released_after_year: Option<u16>,
    released_before_year: Option<u16>,
    include_adult: Option<bool>,
    exclude_owned: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiscoverItem {
    appid: u32,
    name: String,
    score: f64,
    sim_score: f64,
    review_pct: u8,
    review_total: u32,
    release_year: u16,
    is_free: bool,
    tags: Vec<String>,
    reason: String,
    warning: Option<String>,
}

#[tauri::command]
async fn get_discover_feed(
    state: State<'_, AppState>,
    filters: Option<DiscoverFilters>,
) -> Result<Vec<DiscoverItem>, String> {
    let filters = filters.unwrap_or_default();
    let cat = state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("CATALOG_NOT_READY")?;
    let profile = state
        .taste_profile
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("PROFILE_NOT_READY")?;
    let owned: std::collections::HashSet<u32> = state
        .games
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|g| g.appid as u32)
        .collect();

    let min_pct = filters.min_review_pct.unwrap_or(70);
    let min_reviews = filters.min_reviews.unwrap_or(50);
    let include_adult = filters.include_adult.unwrap_or(false);
    let exclude_owned = filters.exclude_owned.unwrap_or(true);

    // Retrieve-then-rank on a blocking thread: top-300 by taste similarity,
    // re-ranked with quality (Bayesian-smoothed) and anti-cluster penalties.
    let items = tauri::async_runtime::spawn_blocking(move || {
    let pool = cat.top_matches(&profile.vector, 300, |_, m| {
        if exclude_owned && owned.contains(&m.appid) {
            return false;
        }
        if !include_adult && m.adult {
            return false;
        }
        if m.review_positive_pct < min_pct || m.review_total < min_reviews {
            return false;
        }
        if filters.require_price.unwrap_or(false) && m.is_free {
            return false;
        }
        if let Some(after) = filters.released_after_year {
            if m.release_year < after {
                return false;
            }
        }
        if let Some(before) = filters.released_before_year {
            if m.release_year == 0 || m.release_year > before {
                return false;
            }
        }
        true
    });

    let mut items: Vec<DiscoverItem> = pool
        .into_iter()
        .map(|(row, _)| {
            let meta = &cat.meta[row as usize];
            let vec = cat.vector_f32(row);
            let scored = taste::score_candidate(
                &profile,
                &vec,
                meta.review_positive_pct,
                meta.review_total,
            );
            DiscoverItem {
                appid: meta.appid,
                name: meta.name.clone(),
                score: scored.score,
                sim_score: scored.sim,
                review_pct: meta.review_positive_pct,
                review_total: meta.review_total,
                release_year: meta.release_year,
                is_free: meta.is_free,
                tags: meta.tags.iter().take(5).cloned().collect(),
                reason: taste::reason_for(&profile, &vec, &meta.tags),
                warning: scored.warning,
            }
        })
        .collect();
    items.sort_by(|a, b| b.score.total_cmp(&a.score));
    items.truncate(100);
    items
    })
    .await
    .map_err(|e| format!("discover scan failed: {e}"))?;
    Ok(items)
}

#[tauri::command]
async fn get_similar_games(
    state: State<'_, AppState>,
    appid: u32,
    count: Option<usize>,
) -> Result<Vec<taste::SimilarGame>, String> {
    let cat = state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("CATALOG_NOT_READY")?;
    let owned: std::collections::HashSet<u32> = state
        .games
        .lock()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|g| g.appid as u32)
        .collect();

    let (row, meta) = cat.get(appid).ok_or("GAME_NOT_IN_CATALOG")?;
    let vec = cat.vector_f32(row);
    let meta = meta.clone();
    let k = count.unwrap_or(12);
    tauri::async_runtime::spawn_blocking(move || {
        taste::similar_games(&cat, &vec, &meta, &owned, k)
    })
    .await
    .map_err(|e| format!("similar-games scan failed: {e}"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TasteFit {
    /// 0-1 taste similarity.
    fit_score: f64,
    matched_tags: Vec<String>,
    nearest_anchors: Vec<String>,
    warning: Option<String>,
}

#[tauri::command]
async fn get_game_taste_fit(state: State<'_, AppState>, appid: u32) -> Result<TasteFit, String> {
    let cat = state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("CATALOG_NOT_READY")?;
    let profile = state
        .taste_profile
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("PROFILE_NOT_READY")?;

    let (row, meta) = cat.get(appid).ok_or("GAME_NOT_IN_CATALOG")?;
    let vec = cat.vector_f32(row);
    let scored = taste::score_candidate(&profile, &vec, meta.review_positive_pct, meta.review_total);

    let user_tags: std::collections::HashSet<&str> =
        profile.top_tags.iter().map(|t| t.tag.as_str()).collect();
    let matched_tags: Vec<String> = meta
        .tags
        .iter()
        .filter(|t| user_tags.contains(t.as_str()))
        .take(4)
        .cloned()
        .collect();

    let mut anchors: Vec<(String, f32)> = profile
        .anchor_games
        .iter()
        .filter(|a| a.weight >= 0.3 && a.appid != appid as u64)
        .filter_map(|a| a.vector.as_ref().map(|v| (a.name.clone(), taste::dot(v, &vec))))
        .collect();
    anchors.sort_by(|a, b| b.1.total_cmp(&a.1));

    Ok(TasteFit {
        fit_score: scored.sim,
        matched_tags,
        nearest_anchors: anchors.into_iter().take(2).map(|(n, _)| n).collect(),
        warning: scored.warning,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WishlistScoredItem {
    appid: u32,
    name: String,
    score: f64,
    sim_score: f64,
    review_pct: u8,
    review_total: u32,
    release_year: u16,
    tags: Vec<String>,
    reason: String,
    warning: Option<String>,
    priority: u32,
    date_added: u64,
    /// Not in the catalog — shown unranked at the bottom.
    unscored: bool,
}

/// Fetch (or reuse ≤1h-old cached) wishlist, join against the catalog, and
/// score every item by taste match. Catalog misses go to an unscored bucket.
#[tauri::command]
async fn get_wishlist_scored(
    state: State<'_, AppState>,
    refresh: Option<bool>,
) -> Result<Vec<WishlistScoredItem>, String> {
    let cfg = config::load_config()?;

    let items = if !refresh.unwrap_or(false) {
        wishlist::load_cached(&cfg.steam_id)
    } else {
        None
    };
    let items = match items {
        Some(items) => items,
        None => {
            let client = state.client.clone();
            let fetched = wishlist::fetch_wishlist(&client, &cfg.steam_id)
                .await
                .map_err(|e| e.to_string())?;
            wishlist::save_cache(&cfg.steam_id, &fetched);
            fetched
        }
    };

    let cat = state
        .catalog
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("CATALOG_NOT_READY")?;
    let profile = state
        .taste_profile
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("PROFILE_NOT_READY")?;

    let mut scored: Vec<WishlistScoredItem> = Vec::new();
    for item in &items {
        match cat.get(item.appid) {
            Some((row, meta)) => {
                let vec = cat.vector_f32(row);
                let s = taste::score_candidate(
                    &profile,
                    &vec,
                    meta.review_positive_pct,
                    meta.review_total,
                );
                scored.push(WishlistScoredItem {
                    appid: meta.appid,
                    name: meta.name.clone(),
                    score: s.score,
                    sim_score: s.sim,
                    review_pct: meta.review_positive_pct,
                    review_total: meta.review_total,
                    release_year: meta.release_year,
                    tags: meta.tags.iter().take(5).cloned().collect(),
                    reason: taste::reason_for(&profile, &vec, &meta.tags),
                    warning: s.warning,
                    priority: item.priority,
                    date_added: item.date_added,
                    unscored: false,
                });
            }
            None => {
                // Unreleased/delisted/too-new — no local name source; the UI
                // links to the store page which shows the real title.
                scored.push(WishlistScoredItem {
                    appid: item.appid,
                    name: format!("App {}", item.appid),
                    score: 0.0,
                    sim_score: 0.0,
                    review_pct: 0,
                    review_total: 0,
                    release_year: 0,
                    tags: Vec::new(),
                    reason: "Not in the catalog yet (unreleased or delisted)".into(),
                    warning: None,
                    priority: item.priority,
                    date_added: item.date_added,
                    unscored: true,
                });
            }
        }
    }
    // Scored items first (by score), unscored bucket at the bottom
    scored.sort_by(|a, b| a.unscored.cmp(&b.unscored).then(b.score.total_cmp(&a.score)));
    Ok(scored)
}

/// LLM-written "what your library says about you" prose. Deterministic profile
/// data goes in; 2-3 paragraphs come out. Cached on disk keyed by a profile
/// hash so it only regenerates when the profile meaningfully changes.
#[tauri::command]
async fn get_taste_prose(
    state: State<'_, AppState>,
    llm_state: State<'_, LlmState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use std::hash::{Hash, Hasher};

    let profile = state
        .taste_profile
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("PROFILE_NOT_READY")?;

    // Stable hash over the parts that matter
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for t in &profile.top_tags {
        t.tag.hash(&mut hasher);
        ((t.weight * 100.0) as u64).hash(&mut hasher);
    }
    for a in &profile.anchor_games {
        a.appid.hash(&mut hasher);
    }
    for c in &profile.anti_clusters {
        c.label.hash(&mut hasher);
        c.bounced.len().hash(&mut hasher);
    }
    let hash = hasher.finish();

    let prose_file = config::cache_dir().join("taste_prose.json");
    if let Ok(data) = std::fs::read_to_string(&prose_file) {
        if let Ok(cached) = serde_json::from_str::<serde_json::Value>(&data) {
            if cached["hash"].as_u64() == Some(hash) {
                if let Some(prose) = cached["prose"].as_str() {
                    return Ok(prose.to_string());
                }
            }
        }
    }

    let data_dir = llm::get_data_dir(&app);
    let setup = llm::check_setup(&data_dir);
    if !(setup.model_ready && setup.server_ready) {
        return Err("AI_NOT_READY".into());
    }
    llm::start_server(&data_dir, &llm_state)?;
    llm::wait_for_server().await?;

    let facts = serde_json::json!({
        "top_tags": profile.top_tags.iter().map(|t| &t.tag).collect::<Vec<_>>(),
        "defining_games": profile.anchor_games.iter().map(|a| &a.name).collect::<Vec<_>>(),
        "bounce_patterns": profile.anti_clusters.iter().map(|c| {
            serde_json::json!({
                "kind": c.label,
                "dropped_games": c.bounced.iter().map(|b| &b.name).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "played_games_count": profile.signal_count,
    });
    let system = "You are a perceptive games critic writing a short personal profile of a player \
        based only on the structured facts provided. Write 2-3 short paragraphs, second person \
        (\"you\"), specific and warm, no lists, no headers. Mention concrete games. If bounce \
        patterns exist, note them kindly. Respond as JSON: {\"prose\": \"...\"}";
    let response = llm::run_taste_prose_inference(system, &facts.to_string()).await?;

    let parsed = serde_json::from_str::<serde_json::Value>(&response)
        .ok()
        .and_then(|v| v["prose"].as_str().map(String::from));

    match parsed {
        Some(prose) => {
            // Cache only well-formed output; malformed responses regenerate next time
            let _ = std::fs::write(
                &prose_file,
                serde_json::json!({"hash": hash, "prose": prose}).to_string(),
            );
            Ok(prose)
        }
        None => Err("AI_PROSE_MALFORMED".into()),
    }
}

// -- Export/Import --

#[tauri::command]
fn export_json(state: State<'_, AppState>) -> Result<String, String> {
    let classifications = state.classifications.lock().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&*classifications)
        .map_err(|e| format!("Failed to serialize: {e}"))
}

pub fn run() {
    // Load persisted data
    let store_cache = cache::load_store_cache();
    let overrides = cache::load_overrides();
    let hltb_cache = cache::load_hltb_cache();
    let saved_classifications: Vec<Classification> =
        cache::load_saved_classifications().into_values().collect();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            client: Client::new(),
            games: Mutex::new(Vec::new()),
            classifications: Mutex::new(saved_classifications),
            store_cache: Mutex::new(store_cache),
            overrides: Mutex::new(overrides),
            hltb_cache: Mutex::new(hltb_cache),
            sync_cancelled: Arc::new(AtomicBool::new(false)),
            hltb_cancelled: Arc::new(AtomicBool::new(false)),
            hltb_fetching: Arc::new(AtomicBool::new(false)),
            store_backfill_running: Arc::new(AtomicBool::new(false)),
            catalog: Mutex::new(None),
            embedder: Mutex::new(None),
            taste_profile: Mutex::new(None),
            taste_loading: Arc::new(AtomicBool::new(false)),
        })
        .manage(LlmState {
            server_process: Mutex::new(None),
            force_cpu: Mutex::new(false),
        })
        .setup(|app| {
            // Load persisted GPU preference
            let data_dir = llm::get_data_dir(app.handle());
            let force_cpu = llm::load_force_cpu(&data_dir);
            let llm_state = app.state::<LlmState>();
            if let Ok(mut guard) = llm_state.force_cpu.lock() {
                *guard = force_cpu;
            }
            // Load catalog + embedder off the main thread
            load_taste_assets_background(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_config,
            save_config,
            fetch_library,
            fetch_store_details,
            classify_games,
            get_classifications,
            set_override,
            remove_override,
            get_overrides,
            get_summary,
            get_couch_profiles,
            check_steam_running,
            get_steam_accounts,
            write_to_steam,
            check_ai_setup,
            setup_ai,
            get_model_tiers,
            set_model_tier,
            delete_model_tier,
            get_gpu_status,
            set_gpu_enabled,
            cancel_sync,
            get_recommendations,
            get_ambiguity_suggestion,
            get_hltb_cache,
            fetch_hltb_data,
            get_cached_library,
            backfill_store_details,
            check_taste_setup,
            get_taste_profile,
            get_discover_feed,
            get_similar_games,
            get_game_taste_fit,
            get_wishlist_scored,
            get_taste_prose,
            export_json,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            let llm_state = app_handle.state::<LlmState>();
            llm::stop_server(&llm_state);
            // Save HLTB cache on exit
            {
                let app_state: tauri::State<AppState> = app_handle.state();
                match app_state.hltb_cache.lock() {
                    Ok(cache) => { let _ = cache::save_hltb_cache(&cache); }
                    Err(_) => eprintln!("[HLTB] Could not save cache on exit (mutex poisoned)"),
                };
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the JS → Rust contract for the Write to Steam options. The frontend
    /// sends camelCase keys; a rename drift here would silently turn the couch
    /// collection opt-in into a no-op.
    #[test]
    fn write_options_deserialize_from_frontend_payload() {
        let opts: WriteOptions = serde_json::from_str(
            r#"{"includeCouch": true, "couchIncludePartial": true}"#,
        )
        .expect("frontend payload must deserialize");
        assert!(opts.include_couch);
        assert!(opts.couch_include_partial);

        let off: WriteOptions =
            serde_json::from_str(r#"{"includeCouch": false, "couchIncludePartial": false}"#)
                .unwrap();
        assert!(!off.include_couch);

        // Absent fields default to off rather than failing the whole command.
        let empty: WriteOptions = serde_json::from_str("{}").unwrap();
        assert!(!empty.include_couch);
        assert!(!empty.couch_include_partial);
    }

    /// The destructive mode must never be what you get by accident.
    #[test]
    fn write_mode_defaults_to_add_new() {
        let empty: WriteOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.mode, WriteMode::AddNew);

        let explicit: WriteOptions =
            serde_json::from_str(r#"{"mode": "replace"}"#).unwrap();
        assert_eq!(explicit.mode, WriteMode::Replace);

        let add: WriteOptions = serde_json::from_str(r#"{"mode": "addNew"}"#).unwrap();
        assert_eq!(add.mode, WriteMode::AddNew);
    }

    #[test]
    fn add_new_keeps_existing_and_appends_unfiled_games() {
        // The user sorted 1, 2, 3 by hand; the rules also want 4 and 5.
        let claimed: HashSet<u64> = [1, 2, 3, 9].into_iter().collect();
        let merged = merge_add_new(&[1, 2, 3], &[3, 4, 5], &claimed);
        assert_eq!(merged, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn add_new_never_files_a_game_another_collection_already_holds() {
        // 9 lives in a different managed collection — leave it where it is.
        let claimed: HashSet<u64> = [9].into_iter().collect();
        let merged = merge_add_new(&[1], &[9, 10], &claimed);
        assert_eq!(merged, vec![1, 10]);
    }

    #[test]
    fn add_new_is_order_stable_and_deduplicates() {
        let merged = merge_add_new(&[5, 1], &[1, 1, 7], &HashSet::new());
        assert_eq!(merged, vec![5, 1, 7]);
    }

    #[test]
    fn add_new_on_an_empty_library_is_just_the_computed_set() {
        // First-ever write: nothing exists, so both modes agree.
        let merged = merge_add_new(&[], &[4, 2], &HashSet::new());
        assert_eq!(merged, vec![4, 2]);
    }
}
