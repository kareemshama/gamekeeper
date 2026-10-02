import { useEffect, useRef, useState } from "react";
import {
  backfillStoreDetails,
  checkConfig,
  fetchLibrary,
  getCachedLibrary,
  fetchStoreDetails,
  classifyGames,
  getClassifications,
  onSyncProgress,
  cancelSync,
  getHltbCache,
  getCouchProfiles,
  isCouchReady,
  fetchHltbData,
  onHltbComplete,
  onTasteReady,
  checkTasteSetup,
  getTasteProfile,
  Classification,
  CategoryKey,
  ConfigStatus,
  CouchFilter,
  CouchProfile,
  DEFAULT_COUCH_FILTER,
  HltbEntry,
  OwnedGame,
  SyncProgress as SyncProgressEvent,
  TasteProfile,
  TasteSetupStatus,
} from "./lib/commands";
import TitleBar from "./components/TitleBar";
import SetupScreen from "./components/SetupScreen";
import GameGrid from "./components/GameGrid";
import Sidebar, { AppView } from "./components/Sidebar";
import DiscoverView from "./components/DiscoverView";
import TasteProfileView from "./components/TasteProfileView";
import WriteToSteam from "./components/WriteToSteam";
import SettingsPanel from "./components/SettingsPanel";
import ChatPanel from "./components/ChatPanel";

type AppPhase = "loading" | "setup" | "syncing" | "ready";

const COUCH_FILTER_KEY = "gamekeeper-couch-filter";

function loadCouchFilter(): CouchFilter {
  try {
    const saved = JSON.parse(localStorage.getItem(COUCH_FILTER_KEY) || "{}");
    return { ...DEFAULT_COUCH_FILTER, ...saved };
  } catch {
    return DEFAULT_COUCH_FILTER;
  }
}

interface SyncState {
  step: string;
  detail: string;
  current: number;
  total: number;
  eta: string;
}

function formatEta(seconds: number): string {
  if (seconds < 60) return `~${Math.ceil(seconds)}s remaining`;
  const mins = Math.floor(seconds / 60);
  const secs = Math.ceil(seconds % 60);
  if (mins < 2) return `~${mins}m ${secs}s remaining`;
  return `~${mins}m remaining`;
}

export default function App() {
  const [phase, setPhase] = useState<AppPhase>("loading");
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [classifications, setClassifications] = useState<Classification[]>([]);
  const [activeCategory, setActiveCategory] = useState<CategoryKey | "ALL">("ALL");
  const [syncState, setSyncState] = useState<SyncState>({
    step: "",
    detail: "",
    current: 0,
    total: 0,
    eta: "",
  });
  const [error, setError] = useState<string | null>(null);
  const [showWriteToSteam, setShowWriteToSteam] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showChat, setShowChat] = useState(false);
  const [hltbCache, setHltbCache] = useState<Record<string, HltbEntry>>({});
  const [hltbFetching, setHltbFetching] = useState(false);
  const [hltbProgress, setHltbProgress] = useState<{ current: number; total: number } | null>(null);
  const [playtimeMap, setPlaytimeMap] = useState<Record<string, number>>({});
  const [couchProfiles, setCouchProfiles] = useState<Record<string, CouchProfile>>({});
  const [couchFilter, setCouchFilter] = useState<CouchFilter>(loadCouchFilter);
  const [view, setView] = useState<AppView>("library");
  const [tasteSetup, setTasteSetup] = useState<TasteSetupStatus | null>(null);
  const [tasteProfile, setTasteProfile] = useState<TasteProfile | null>(null);
  const [tasteError, setTasteError] = useState<string | null>(null);

  const tasteRetries = useRef(0);

  // Persist the couch filter so a TV setup stays set up between launches
  useEffect(() => {
    localStorage.setItem(COUCH_FILTER_KEY, JSON.stringify(couchFilter));
  }, [couchFilter]);

  /** Couch profiles come from cached store data — cheap, offline, no network. */
  function refreshCouchProfiles() {
    getCouchProfiles().then(setCouchProfiles).catch(() => {});
  }

  async function refreshTasteProfile(force?: boolean) {
    try {
      setTasteSetup(await checkTasteSetup());
      const profile = await getTasteProfile(force);
      setTasteProfile(profile);
      setTasteError(null);
      tasteRetries.current = 0;
    } catch (e) {
      const msg = String(e);
      setTasteError(msg);
      // The taste-ready event can fire before this webview attached its
      // listener, and library hydration races catalog loading — retry with
      // backoff instead of relying on a one-shot event.
      const transient = msg.includes("CATALOG_NOT_READY") || msg.includes("LIBRARY_NOT_LOADED");
      if (transient && tasteRetries.current < 15) {
        tasteRetries.current += 1;
        setTimeout(() => refreshTasteProfile(force), 2000);
      }
    }
  }

  // Track when each sync step started for ETA calculation
  const stepStartTime = useRef<number>(0);
  const lastStep = useRef<string>("");

  useEffect(() => {
    initialize();

    // Load HLTB cache on startup
    getHltbCache().then(setHltbCache).catch(() => {});

    const unlistenHltb = onHltbComplete((data) => {
      setHltbFetching(false);
      setHltbProgress(null);
      // Reload cache after fetch completes
      getHltbCache().then(setHltbCache).catch(() => {});
    });

    // Catalog + embed model finish loading shortly after launch
    const unlistenTaste = onTasteReady(() => {
      refreshTasteProfile();
    });

    const unlisten = onSyncProgress((p: SyncProgressEvent) => {
      // Reset timer when step changes
      if (p.step !== lastStep.current) {
        stepStartTime.current = Date.now();
        lastStep.current = p.step;
      }

      // Calculate ETA
      let eta = "";
      const elapsed = (Date.now() - stepStartTime.current) / 1000;
      if (p.current > 1 && p.total > 0 && elapsed > 2) {
        const rate = (p.current - 1) / elapsed; // items per second
        const remaining = p.total - p.current;
        if (rate > 0) {
          eta = formatEta(remaining / rate);
        }
      }

      setSyncState({
        step: p.step,
        detail: `${p.step} (${p.current} / ${p.total})`,
        current: p.current,
        total: p.total,
        eta,
      });

      // Track HLTB fetch progress separately for the ready-state banner
      if (p.step === "Fetching completion times") {
        setHltbProgress({ current: p.current, total: p.total });
        setHltbFetching(true);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
      unlistenHltb.then((fn) => fn());
      unlistenTaste.then((fn) => fn());
    };
  }, []);

  // Refresh HLTB cache periodically only while fetching
  useEffect(() => {
    if (!hltbFetching) return;
    const interval = setInterval(() => {
      getHltbCache().then(setHltbCache).catch(() => {});
    }, 10000);
    return () => clearInterval(interval);
  }, [hltbFetching]);

  function buildPlaytimeMap(games: OwnedGame[]) {
    const map: Record<string, number> = {};
    for (const g of games) {
      map[String(g.appid)] = g.playtime_hours;
    }
    setPlaytimeMap(map);
  }

  async function initialize() {
    try {
      const status = await checkConfig();
      setConfigStatus(status);
      if (!status.configured) {
        setPhase("setup");
        return;
      }

      // Load any existing classifications
      const existing = await getClassifications();
      if (existing.length > 0) {
        setClassifications(existing);
        setPhase("ready");
        refreshCouchProfiles();
        // Cold start: hydrate playtime from the cached library (disk only, never network)
        getCachedLibrary()
          .then((games) => {
            buildPlaytimeMap(games);
            // Library is in backend state now — profile can compute
            refreshTasteProfile();
          })
          .catch(() => {});
        // Silently backfill v2 store fields for pre-taste-engine caches
        backfillStoreDetails().catch(() => {});
      } else {
        setPhase("setup");
      }
    } catch (e) {
      setError(String(e));
      setPhase("setup");
    }
  }

  async function handleSetupComplete() {
    setPhase("syncing");
    setError(null);
    try {
      setSyncState({ step: "Fetching library", detail: "Getting your games from Steam...", current: 0, total: 0, eta: "" });
      const library = await fetchLibrary();
      buildPlaytimeMap(library);

      setSyncState({ step: "Fetching details", detail: "Loading store data for each game...", current: 0, total: 0, eta: "" });
      await fetchStoreDetails();

      setSyncState({ step: "Classifying", detail: "Sorting your games into categories...", current: 0, total: 0, eta: "" });
      const results = await classifyGames();
      setClassifications(results);

      setPhase("ready");
      refreshCouchProfiles();

      // Fresh sync data (incl. last-played times) → recompute taste profile
      refreshTasteProfile(true);

      // Start HLTB background fetch after sync
      setHltbFetching(true);
      fetchHltbData().catch(() => setHltbFetching(false));
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        // User cancelled — go back to ready if we have data, otherwise setup
        const existing = await getClassifications().catch(() => []);
        if (existing.length > 0) {
          setClassifications(existing);
          setPhase("ready");
        } else {
          setPhase("setup");
        }
      } else {
        setError(msg);
        setPhase("setup");
      }
    }
  }

  async function handleResync() {
    setPhase("syncing");
    setError(null);
    try {
      setSyncState({ step: "Fetching library", detail: "Refreshing your games...", current: 0, total: 0, eta: "" });
      const library = await fetchLibrary();
      buildPlaytimeMap(library);

      setSyncState({ step: "Fetching details", detail: "Updating store data...", current: 0, total: 0, eta: "" });
      await fetchStoreDetails();

      setSyncState({ step: "Classifying", detail: "Re-sorting your games...", current: 0, total: 0, eta: "" });
      const results = await classifyGames();
      setClassifications(results);

      setPhase("ready");
      refreshCouchProfiles();

      // Start HLTB background fetch after resync
      setHltbFetching(true);
      fetchHltbData().catch(() => setHltbFetching(false));
    } catch (e) {
      const msg = String(e);
      if (msg.includes("cancelled")) {
        setPhase("ready");
      } else {
        setError(msg);
        setPhase("ready");
      }
    }
  }

  async function handleCancelSync() {
    await cancelSync();
  }

  /** Couch-ready under the current sub-options, ignoring the on/off switch. */
  function isCouchPick(appid: number): boolean {
    const profile = couchProfiles[String(appid)];
    if (!isCouchReady(profile, couchFilter.includePartial)) return false;
    if (couchFilter.splitScreenOnly && !profile.splitScreen) return false;
    return true;
  }

  const inCategory =
    activeCategory === "ALL"
      ? classifications
      : classifications.filter((c) => c.category === activeCategory);

  const filteredGames = couchFilter.on
    ? inCategory.filter((c) => isCouchPick(c.appid))
    : inCategory;

  // Shown next to the sidebar toggle, so the number is useful before switching on
  const couchCount = inCategory.filter((c) => isCouchPick(c.appid)).length;

  const counts = {
    ALL: classifications.length,
    COMPLETED: classifications.filter((c) => c.category === "COMPLETED").length,
    IN_PROGRESS: classifications.filter((c) => c.category === "IN_PROGRESS").length,
    ENDLESS: classifications.filter((c) => c.category === "ENDLESS").length,
    NOT_A_GAME: classifications.filter((c) => c.category === "NOT_A_GAME").length,
  };

  const syncPercent = syncState.total > 0
    ? Math.round((syncState.current / syncState.total) * 100)
    : 0;

  if (phase === "loading") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex items-center justify-center flex-1">
          <div className="text-steam-text-dim text-lg">Loading...</div>
        </div>
      </div>
    );
  }

  if (phase === "setup") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex-1 overflow-auto">
          <SetupScreen
            onComplete={handleSetupComplete}
            error={error}
            hasExistingConfig={configStatus?.configured ?? false}
          />
        </div>
      </div>
    );
  }

  if (phase === "syncing") {
    return (
      <div className="flex flex-col h-screen">
        <TitleBar />
        <div className="flex flex-col items-center justify-center flex-1 gap-6">
          <div className="w-12 h-12 border-4 border-steam-blue border-t-transparent rounded-full animate-spin" />
          <div className="text-center w-80">
            <div className="text-xl font-semibold text-white mb-2">
              {syncState.step}
            </div>
            <div className="text-steam-text-dim text-sm mb-4">
              {syncState.detail}
            </div>
            {syncState.total > 0 && (
              <div>
                <div className="w-full rounded-full h-2 bg-steam-surface-light">
                  <div
                    className="h-2 rounded-full bg-steam-blue transition-all duration-300"
                    style={{ width: `${syncPercent}%` }}
                  />
                </div>
                <div className="flex justify-between text-xs text-steam-text-dim mt-2">
                  <span>{syncState.current} / {syncState.total} — {syncPercent}%</span>
                  {syncState.eta && <span>{syncState.eta}</span>}
                </div>
              </div>
            )}
            <button
              onClick={handleCancelSync}
              className="mt-6 py-2 px-6 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light border border-steam-border transition-colors"
            >
              Stop syncing
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen">
      <TitleBar />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar
          activeView={view}
          onViewChange={setView}
          activeCategory={activeCategory}
          onCategoryChange={setActiveCategory}
          counts={counts}
          couchOn={couchFilter.on}
          onCouchToggle={() => setCouchFilter((f) => ({ ...f, on: !f.on }))}
          couchCount={couchCount}
          onResync={handleResync}
          onWriteToSteam={() => setShowWriteToSteam(true)}
          onSettings={() => setShowSettings(true)}
          onChat={() => setShowChat(true)}
        />
        <main className="flex-1 overflow-hidden flex flex-col">
          {view === "library" && (
            <GameGrid
              games={filteredGames}
              hltbCache={hltbCache}
              hltbFetching={hltbFetching}
              hltbProgress={hltbProgress}
              playtimeMap={playtimeMap}
              couchProfiles={couchProfiles}
              couchFilter={couchFilter}
              onCouchFilterChange={setCouchFilter}
              onOverrideChange={async () => {
                const results = await classifyGames();
                setClassifications(results);
                refreshCouchProfiles();
                // Category overrides change taste weights — recompute
                refreshTasteProfile(true);
              }}
            />
          )}
          {view === "discover" && (
            <DiscoverView
              tasteSetup={tasteSetup}
              profileReady={tasteProfile !== null}
              lowConfidence={tasteProfile?.confidence === "low"}
              signalCount={tasteProfile?.signalCount ?? 0}
            />
          )}
          {view === "taste" && (
            <TasteProfileView
              profile={tasteProfile}
              tasteSetup={tasteSetup}
              error={tasteError}
            />
          )}
        </main>
      </div>
      {showWriteToSteam && (
        <WriteToSteam
          onClose={() => setShowWriteToSteam(false)}
          totalGames={classifications.length}
          couchCount={
            classifications.filter(
              (c) => c.category !== "NOT_A_GAME" && isCouchReady(couchProfiles[String(c.appid)], false)
            ).length
          }
          couchCountWithPartial={
            classifications.filter(
              (c) => c.category !== "NOT_A_GAME" && isCouchReady(couchProfiles[String(c.appid)], true)
            ).length
          }
        />
      )}
      {showSettings && (
        <SettingsPanel
          onClose={() => setShowSettings(false)}
          onConfigUpdated={() => {}}
        />
      )}
      {showChat && <ChatPanel onClose={() => setShowChat(false)} />}
    </div>
  );
}
