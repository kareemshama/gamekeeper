import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// -- Types --

export type CategoryKey =
  | "COMPLETED"
  | "IN_PROGRESS"
  | "ENDLESS"
  | "NOT_A_GAME";

export interface OwnedGame {
  appid: number;
  name: string;
  playtime_hours: number;
  /** Hours played in the last two weeks (0 when not playing). */
  playtime_2weeks_hours: number;
  /** Unix timestamp of last play session; 0 = never played / unknown. */
  rtime_last_played: number;
  achievements?: {
    total: number;
    achieved: number;
    percentage: number;
    names_achieved: string[];
  };
}

export interface Classification {
  appid: number;
  name: string;
  category: CategoryKey;
  confidence: string;
  reason: string;
}

export interface ConfigStatus {
  configured: boolean;
  steam_id: string | null;
}

export interface CategorySummary {
  completed: number;
  in_progress: number;
  endless: number;
  not_a_game: number;
  total: number;
}

// -- Commands --

export function checkConfig(): Promise<ConfigStatus> {
  return invoke("check_config");
}

export function saveConfig(
  apiKey: string,
  steamId: string
): Promise<void> {
  return invoke("save_config", { apiKey, steamId });
}

export function fetchLibrary(): Promise<OwnedGame[]> {
  return invoke("fetch_library");
}

export function fetchStoreDetails(): Promise<void> {
  return invoke("fetch_store_details");
}

export function classifyGames(): Promise<Classification[]> {
  return invoke("classify_games");
}

export function getClassifications(): Promise<Classification[]> {
  return invoke("get_classifications");
}

export function setOverride(
  appid: string,
  category: CategoryKey
): Promise<void> {
  return invoke("set_override", { appid, category });
}

export function removeOverride(appid: string): Promise<void> {
  return invoke("remove_override", { appid });
}

export function getOverrides(): Promise<Record<string, string>> {
  return invoke("get_overrides");
}

export function getSummary(): Promise<CategorySummary> {
  return invoke("get_summary");
}

// -- Couch / TV play style --

export type ControllerSupport = "full" | "partial" | "none" | "unknown";

export interface CouchProfile {
  appid: number;
  /** Gamepad support as reported by Steam's store categories. */
  controller: ControllerSupport;
  /** Streams to a TV via Steam Remote Play / Steam Link. */
  remotePlayTv: boolean;
  /** Local multiplayer on one screen. */
  splitScreen: boolean;
  remotePlayTogether: boolean;
  /** VR headset required — never a sofa game. */
  vrOnly: boolean;
  /** Full gamepad support and not VR-only: never needs a keyboard. */
  tvReady: boolean;
  score: number;
  /** Short display chips, strongest first. */
  badges: string[];
}

/** Couch profiles for every classified game, keyed by app id. Derived from the
 *  cached store data — offline and instant, no fetch needed. */
export function getCouchProfiles(): Promise<Record<string, CouchProfile>> {
  return invoke("get_couch_profiles");
}

/** Does this game clear the couch bar? Mirrors `CouchProfile::qualifies` in Rust. */
export function isCouchReady(
  profile: CouchProfile | undefined,
  includePartial: boolean
): boolean {
  if (!profile || profile.vrOnly) return false;
  if (profile.controller === "full") return true;
  return includePartial && profile.controller === "partial";
}

/** Couch / TV filter state — layers on top of the selected collection. */
export interface CouchFilter {
  on: boolean;
  /** Accept "Partial Controller Support" too, not just full. */
  includePartial: boolean;
  /** Only games with local split-screen multiplayer. */
  splitScreenOnly: boolean;
}

export const DEFAULT_COUCH_FILTER: CouchFilter = {
  on: false,
  includePartial: false,
  splitScreenOnly: false,
};

export const CONTROLLER_LABELS: Record<ControllerSupport, string> = {
  full: "Full controller support",
  partial: "Partial controller support",
  none: "Keyboard & mouse",
  unknown: "Controller support unknown",
};

// -- Steam Collections --

export interface SteamAccount {
  id: string;
  path: string;
}

export function checkSteamRunning(): Promise<boolean> {
  return invoke("check_steam_running");
}

export function getSteamAccounts(): Promise<SteamAccount[]> {
  return invoke("get_steam_accounts");
}

/** How a write treats games already filed in the managed collections. */
export type WriteMode = "addNew" | "replace";

export interface WriteOptions {
  /** Also write the "Controller Friendly" collection for a TV / couch setup. */
  includeCouch: boolean;
  /** Let partial gamepad support into that collection. */
  couchIncludePartial: boolean;
  /** "addNew" never removes a game; "replace" overwrites with Gamekeeper's sorting. */
  mode: WriteMode;
}

/** What the write actually put into Steam, straight from the backend. */
export interface WriteReport {
  /** `[collection name, game count]` in the order written. */
  collections: [string, number][];
  /** Old `SBO:`-prefixed collections deleted during this write. */
  removed: string[];
}

export function writeToSteam(
  accountPath: string,
  options?: WriteOptions
): Promise<WriteReport> {
  return invoke("write_to_steam", { accountPath, options });
}

// -- AI (bundled llama-server) --

export interface SetupStatus {
  modelReady: boolean;
  serverReady: boolean;
  /** Id of the active model tier, null when none installed. */
  activeTier: string | null;
}

export interface ModelTierInfo {
  id: string;
  label: string;
  sizeBytes: number;
  installed: boolean;
  active: boolean;
  recommended: boolean;
}

export interface ModelTiersResponse {
  tiers: ModelTierInfo[];
  vramMb: number | null;
  ramMb: number;
}

export interface DownloadProgress {
  stage: string;
  downloaded: number;
  total: number;
  percent: number;
}

export interface GpuStatus {
  gpuDetected: boolean;
  cudaBuild: boolean;
  usingGpu: boolean;
}

export interface GameRecommendation {
  appid: number;
  title: string;
  reason: string;
}

export interface RecommendResponse {
  picks: GameRecommendation[];
  used_llm: boolean;
  message: string;
}

export interface AmbiguityResponse {
  suggested_category: string;
  rationale: string;
  used_llm: boolean;
}

export function checkAiSetup(): Promise<SetupStatus> {
  return invoke("check_ai_setup");
}

/** Download llama-server + the given tier's model (default: recommended tier). */
export function setupAi(tier?: string): Promise<void> {
  return invoke("setup_ai", { tier });
}

export function getModelTiers(): Promise<ModelTiersResponse> {
  return invoke("get_model_tiers");
}

/** Switch the active (already-installed) tier; restarts the AI server. */
export function setModelTier(tier: string): Promise<void> {
  return invoke("set_model_tier", { tier });
}

/** Delete a tier's model file to free disk space. */
export function deleteModelTier(tier: string): Promise<void> {
  return invoke("delete_model_tier", { tier });
}

/** LLM-written taste profile prose. Rejects with AI_NOT_READY when no model. */
export function getTasteProse(): Promise<string> {
  return invoke("get_taste_prose");
}

export function getGpuStatus(): Promise<GpuStatus> {
  return invoke("get_gpu_status");
}

export function setGpuEnabled(enabled: boolean): Promise<void> {
  return invoke("set_gpu_enabled", { enabled });
}

export function onDownloadProgress(
  callback: (progress: DownloadProgress) => void
): Promise<UnlistenFn> {
  return listen<DownloadProgress>("download-progress", (event) => {
    callback(event.payload);
  });
}

export interface SyncProgress {
  step: string;
  current: number;
  total: number;
}

export function onSyncProgress(
  callback: (progress: SyncProgress) => void
): Promise<UnlistenFn> {
  return listen<SyncProgress>("sync-progress", (event) => {
    callback(event.payload);
  });
}

export function cancelSync(): Promise<void> {
  return invoke("cancel_sync");
}

export interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

export function getRecommendations(
  message: string,
  history: ChatMessage[]
): Promise<RecommendResponse> {
  return invoke("get_recommendations", { request: { message, history } });
}

export function getAmbiguitySuggestion(
  appid: number
): Promise<AmbiguityResponse> {
  return invoke("get_ambiguity_suggestion", { request: { appid } });
}

export function exportJson(): Promise<string> {
  return invoke("export_json");
}

// -- HLTB --

export interface HltbEntry {
  main_story_hours: number | null;
  main_extra_hours: number | null;
  completionist_hours: number | null;
  hltb_game_id: string | null;
  match_status: string;
  fetched_at: number;
}

export interface HltbComplete {
  matched: number;
  total: number;
  new_matches: number;
}

export function getHltbCache(): Promise<Record<string, HltbEntry>> {
  return invoke("get_hltb_cache");
}

export function fetchHltbData(): Promise<void> {
  return invoke("fetch_hltb_data");
}

/** Cached library with no freshness check and no network fallback — for
 *  hydrating playtime display on cold start. Empty array when no cache. */
export function getCachedLibrary(): Promise<OwnedGame[]> {
  return invoke("get_cached_library");
}

/** Backfill v2 store fields (descriptions etc.) for pre-taste-engine cache
 *  entries. Runs silently in the background; no-ops when nothing is missing. */
export function backfillStoreDetails(): Promise<void> {
  return invoke("backfill_store_details");
}

// -- Taste engine --

export interface TasteSetupStatus {
  catalogInstalled: boolean;
  /** yyyymmdd of the catalog's source dataset, e.g. 20260901. */
  catalogDatasetDate: number | null;
  catalogGameCount: number | null;
  embedModelInstalled: boolean;
  loading: boolean;
}

export interface TagAffinity {
  tag: string;
  /** 0-1, top tag = 1. */
  weight: number;
  exampleAppids: number[];
}

export interface AnchorGame {
  appid: number;
  name: string;
  weight: number;
}

export interface BouncedGame {
  appid: number;
  name: string;
  playtimeHours: number;
  lastPlayed: number;
  kind: "bounced" | "abandoned";
}

export interface AntiCluster {
  label: string;
  tags: string[];
  bounced: BouncedGame[];
  /** 0-1, saturates at 6 effective bounces. */
  strength: number;
}

export interface TasteProfile {
  topTags: TagAffinity[];
  anchorGames: AnchorGame[];
  antiClusters: AntiCluster[];
  signalCount: number;
  confidence: "low" | "medium" | "high";
  computedAt: number;
}

export interface DiscoverFilters {
  minReviewPct?: number;
  minReviews?: number;
  requirePrice?: boolean;
  releasedAfterYear?: number;
  releasedBeforeYear?: number;
  includeAdult?: boolean;
  excludeOwned?: boolean;
}

export interface DiscoverItem {
  appid: number;
  name: string;
  score: number;
  simScore: number;
  reviewPct: number;
  reviewTotal: number;
  releaseYear: number;
  isFree: boolean;
  tags: string[];
  reason: string;
  warning: string | null;
}

export interface SimilarGame {
  appid: number;
  name: string;
  similarity: number;
  tags: string[];
  reviewPositivePct: number;
  reviewTotal: number;
  /** Cross-genre find — primary tag differs from the source game's. */
  nonObvious: boolean;
  owned: boolean;
}

export interface TasteFit {
  /** 0-1 taste similarity. */
  fitScore: number;
  matchedTags: string[];
  nearestAnchors: string[];
  warning: string | null;
}

export function checkTasteSetup(): Promise<TasteSetupStatus> {
  return invoke("check_taste_setup");
}

export function getTasteProfile(force?: boolean): Promise<TasteProfile> {
  return invoke("get_taste_profile", { force });
}

export function getDiscoverFeed(filters?: DiscoverFilters): Promise<DiscoverItem[]> {
  return invoke("get_discover_feed", { filters });
}

export function getSimilarGames(appid: number, count?: number): Promise<SimilarGame[]> {
  return invoke("get_similar_games", { appid, count });
}

export function getGameTasteFit(appid: number): Promise<TasteFit> {
  return invoke("get_game_taste_fit", { appid });
}

export interface WishlistScoredItem {
  appid: number;
  name: string;
  score: number;
  simScore: number;
  reviewPct: number;
  reviewTotal: number;
  releaseYear: number;
  tags: string[];
  reason: string;
  warning: string | null;
  priority: number;
  dateAdded: number;
  /** Not in the catalog — shown unranked at the bottom. */
  unscored: boolean;
}

/** Wishlist ranked by taste match. Rejects with WISHLIST_EMPTY_OR_PRIVATE when
 *  the profile is private or the wishlist is empty. Cached for 1h unless refresh. */
export function getWishlistScored(refresh?: boolean): Promise<WishlistScoredItem[]> {
  return invoke("get_wishlist_scored", { refresh });
}

/** Fires once the catalog + embed model finish loading at startup.
 *  Payload: whether the catalog loaded successfully. */
export function onTasteReady(
  callback: (catalogOk: boolean) => void
): Promise<UnlistenFn> {
  return listen<boolean>("taste-ready", (event) => {
    callback(event.payload);
  });
}

export function onHltbComplete(
  callback: (data: HltbComplete) => void
): Promise<UnlistenFn> {
  return listen<HltbComplete>("hltb-complete", (event) => {
    callback(event.payload);
  });
}

// -- Helpers --

export const CATEGORY_LABELS: Record<CategoryKey, string> = {
  COMPLETED: "Completed",
  IN_PROGRESS: "In Progress / Backlog",
  ENDLESS: "Endless",
  NOT_A_GAME: "Not a Game",
};

export const CATEGORY_COLORS: Record<CategoryKey, string> = {
  COMPLETED: "#4CAF50",
  IN_PROGRESS: "#FFC107",
  ENDLESS: "#2196F3",
  NOT_A_GAME: "#9E9E9E",
};

export const STEAM_HEADER_URL = (appid: number) =>
  `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/header.jpg`;

export const STEAM_CAPSULE_URL = (appid: number) =>
  `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/capsule_231x87.jpg`;
