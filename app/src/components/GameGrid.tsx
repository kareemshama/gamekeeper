import { useState, useEffect, useMemo } from "react";
import {
  Classification,
  CategoryKey,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
  STEAM_HEADER_URL,
  CouchFilter,
  CouchProfile,
  HltbEntry,
  setOverride,
} from "../lib/commands";
import GameDetail from "./GameDetail";
import { GamepadIcon } from "./icons";

const TIPS_DISMISSED_KEY = "steam-backlog-tips-dismissed";
const HLTB_FILTER_KEY = "gamekeeper-hltb-filter";

interface Props {
  games: Classification[];
  hltbCache: Record<string, HltbEntry>;
  hltbFetching: boolean;
  hltbProgress: { current: number; total: number } | null;
  playtimeMap: Record<string, number>;
  couchProfiles: Record<string, CouchProfile>;
  couchFilter: CouchFilter;
  onCouchFilterChange: (next: CouchFilter) => void;
  onOverrideChange: () => void;
}

export default function GameGrid({
  games,
  hltbCache,
  hltbFetching,
  hltbProgress,
  playtimeMap,
  couchProfiles,
  couchFilter,
  onCouchFilterChange,
  onOverrideChange,
}: Props) {
  const [search, setSearch] = useState("");
  const [selectedGame, setSelectedGame] = useState<Classification | null>(null);
  const [showTips, setShowTips] = useState(false);
  const [shortGamesOnly, setShortGamesOnly] = useState(() => {
    try { return JSON.parse(localStorage.getItem(HLTB_FILTER_KEY) || "{}").on ?? false; } catch { return false; }
  });
  const [maxHours, setMaxHours] = useState(() => {
    try { return JSON.parse(localStorage.getItem(HLTB_FILTER_KEY) || "{}").max ?? 10; } catch { return 10; }
  });
  const [maxHoursInput, setMaxHoursInput] = useState(() => String(maxHours));
  const [includeUnknown, setIncludeUnknown] = useState(() => {
    try { return JSON.parse(localStorage.getItem(HLTB_FILTER_KEY) || "{}").unknown ?? false; } catch { return false; }
  });

  // Persist filter settings
  useEffect(() => {
    localStorage.setItem(HLTB_FILTER_KEY, JSON.stringify({ on: shortGamesOnly, max: maxHours, unknown: includeUnknown }));
  }, [shortGamesOnly, maxHours, includeUnknown]);

  useEffect(() => {
    const dismissed = localStorage.getItem(TIPS_DISMISSED_KEY);
    if (!dismissed) setShowTips(true);
  }, []);

  const sorted = useMemo(() => {
    const searchActive = search.trim().length > 0;
    let filtered = searchActive
      ? games.filter((g) =>
          g.name.toLowerCase().includes(search.toLowerCase())
        )
      : games;

    // Apply HLTB filter only when toggled on AND no search is active
    if (shortGamesOnly && !searchActive) {
      filtered = filtered.filter((g) => {
        const hltb = hltbCache[String(g.appid)];
        if (!hltb || hltb.main_story_hours == null) return includeUnknown;
        return hltb.main_story_hours <= maxHours;
      });
    }

    return [...filtered].sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase())
    );
  }, [games, search, shortGamesOnly, maxHours, includeUnknown, hltbCache]);

  // Games Steam gave us no categories for can't be judged either way
  const unknownControllerCount = useMemo(
    () =>
      Object.values(couchProfiles).filter((p) => p.controller === "unknown").length,
    [couchProfiles]
  );

  return (
    <div className="flex flex-col h-full">
      {/* Search bar + HLTB filter */}
      <div className="p-4 border-b border-steam-border">
        <div className="flex items-center gap-3">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search games..."
            className="flex-1 px-3 py-2 rounded-lg bg-steam-surface border border-steam-border text-white placeholder-steam-text-dim focus:border-steam-blue focus:outline-none text-sm"
          />
          <button
            onClick={() => setShortGamesOnly(!shortGamesOnly)}
            className={`px-3 py-2 rounded-lg text-xs font-medium transition-colors whitespace-nowrap ${
              shortGamesOnly
                ? "bg-steam-blue text-white"
                : "bg-steam-surface border border-steam-border text-steam-text-dim hover:text-white"
            }`}
            aria-label="Short games only filter"
            role="switch"
            aria-checked={shortGamesOnly}
          >
            Short games
          </button>
          <button
            onClick={() => onCouchFilterChange({ ...couchFilter, on: !couchFilter.on })}
            className={`px-3 py-2 rounded-lg text-xs font-medium transition-colors whitespace-nowrap flex items-center gap-1.5 ${
              couchFilter.on
                ? "bg-steam-blue text-white"
                : "bg-steam-surface border border-steam-border text-steam-text-dim hover:text-white"
            }`}
            aria-label="Couch and TV filter"
            role="switch"
            aria-checked={couchFilter.on}
            title="Gamepad-friendly games you can play from the sofa"
          >
            <GamepadIcon />
            Couch / TV
          </button>
        </div>

        {/* Couch filter controls (visible when toggle is on) */}
        {couchFilter.on && (
          <div className="mt-2 flex flex-wrap items-center gap-4 p-2 rounded-lg bg-steam-bg">
            <span className="text-xs text-steam-text-dim">
              {couchFilter.includePartial
                ? "Full or partial controller support"
                : "Full controller support"}
            </span>
            <label className="flex items-center gap-1.5 text-xs text-steam-text-dim cursor-pointer">
              <input
                type="checkbox"
                checked={couchFilter.includePartial}
                onChange={(e) =>
                  onCouchFilterChange({ ...couchFilter, includePartial: e.target.checked })
                }
                className="accent-steam-blue"
              />
              Include partial support
            </label>
            <label className="flex items-center gap-1.5 text-xs text-steam-text-dim cursor-pointer">
              <input
                type="checkbox"
                checked={couchFilter.splitScreenOnly}
                onChange={(e) =>
                  onCouchFilterChange({ ...couchFilter, splitScreenOnly: e.target.checked })
                }
                className="accent-steam-blue"
              />
              Split screen only
            </label>
          </div>
        )}

        {/* HLTB filter controls (visible when toggle is on) */}
        {shortGamesOnly && (
          <div className="mt-2 flex flex-wrap items-center gap-3 p-2 rounded-lg bg-steam-bg">
            <div className="flex items-center gap-2">
              <input
                type="range"
                min="1"
                max="100"
                value={Math.min(maxHours, 100)}
                onChange={(e) => {
                  const val = Number(e.target.value);
                  setMaxHours(val);
                  setMaxHoursInput(String(val));
                }}
                className="w-28 accent-steam-blue"
                aria-label="Maximum game length in hours"
              />
              <span className="text-xs text-steam-text-dim">&le;</span>
              <input
                type="text"
                value={maxHoursInput}
                onChange={(e) => {
                  setMaxHoursInput(e.target.value);
                  const val = Number(e.target.value);
                  if (!isNaN(val) && isFinite(val) && val > 0) setMaxHours(val);
                }}
                onBlur={() => setMaxHoursInput(String(maxHours))}
                className="w-12 px-1 py-0.5 rounded bg-steam-surface border border-steam-border text-steam-blue text-xs text-center font-semibold"
              />
              <span className="text-xs text-steam-text-dim">hours</span>
            </div>
            <label className="flex items-center gap-1.5 text-xs text-steam-text-dim cursor-pointer">
              <input
                type="checkbox"
                checked={includeUnknown}
                onChange={(e) => setIncludeUnknown(e.target.checked)}
                className="accent-steam-blue"
              />
              Include unknown
            </label>
          </div>
        )}

        <div className="mt-2 flex items-center gap-2 text-xs text-steam-text-dim">
          <span>{sorted.length} game{sorted.length !== 1 ? "s" : ""}</span>
          {couchFilter.on && unknownControllerCount > 0 && (
            <span title="Usually delisted or region-locked games — Steam's store API returns nothing for them.">
              {unknownControllerCount} hidden &mdash; Steam lists no controller data for them.
            </span>
          )}
          {hltbFetching && hltbProgress && (
            <span className="text-steam-blue">
              Fetching completion times... {hltbProgress.current}/{hltbProgress.total}
              {hltbProgress.total > 0 && ` (~${Math.ceil((hltbProgress.total - hltbProgress.current) / 3 / 60)}m remaining)`}
            </span>
          )}
          {shortGamesOnly && hltbFetching && !hltbProgress && (
            <span className="text-yellow-500">HLTB data still loading.</span>
          )}
          {shortGamesOnly && !hltbFetching && Object.keys(hltbCache).length === 0 && (
            <span>No HLTB data yet. Sync your library to fetch completion times.</span>
          )}
        </div>
      </div>

      {/* Welcome tips */}
      {showTips && (
        <div className="mx-4 mt-3 p-4 rounded-lg bg-steam-blue/10 border border-steam-blue/25 animate-fadeIn">
          <div className="flex items-start justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-white mb-2">Welcome to Gamekeeper</h3>
              <ul className="text-xs text-steam-text-dim space-y-1.5">
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">Click any game</strong> to view details, see why it was classified, or change its category</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">Sidebar categories</strong> filter your library — counts update as you override games</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">"What should I play?"</strong> opens the AI assistant for personalized recommendations</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">"Write to Steam"</strong> pushes your collections into Steam so they appear across devices</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">"Re-sync library"</strong> refreshes your games from Steam — useful after buying new games</span>
                </li>
                <li className="flex items-start gap-2">
                  <span className="text-steam-blue shrink-0 mt-0.5">&#8226;</span>
                  <span><strong className="text-steam-text">Settings</strong> lets you update your Steam config, download the AI model, export data, and more</span>
                </li>
              </ul>
            </div>
            <button
              onClick={() => {
                setShowTips(false);
                localStorage.setItem(TIPS_DISMISSED_KEY, "1");
              }}
              className="text-steam-text-dim hover:text-white transition-colors shrink-0 p-1"
              title="Dismiss"
            >
              <svg width="14" height="14" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M1 1l12 12M13 1L1 13" />
              </svg>
            </button>
          </div>
        </div>
      )}

      {/* Grid */}
      <div className="flex-1 overflow-y-auto p-4">
        <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
          {sorted.map((game) => (
            <GameCard
              key={game.appid}
              game={game}
              hltb={hltbCache[String(game.appid)]}
              couch={couchProfiles[String(game.appid)]}
              onClick={() => setSelectedGame(game)}
            />
          ))}
        </div>

        {sorted.length === 0 && (
          <div className="flex items-center justify-center h-48 text-steam-text-dim">
            {search ? "No games match your search." : "No games in this category."}
          </div>
        )}
      </div>

      {/* Detail drawer */}
      {selectedGame && (
        <GameDetail
          game={selectedGame}
          hltb={hltbCache[String(selectedGame.appid)]}
          couch={couchProfiles[String(selectedGame.appid)]}
          playtimeHours={playtimeMap[String(selectedGame.appid)] ?? 0}
          onClose={() => setSelectedGame(null)}
          onOverride={async (appid, category) => {
            await setOverride(String(appid), category);
            onOverrideChange();
            setSelectedGame(null);
          }}
        />
      )}
    </div>
  );
}

function GameCard({
  game,
  hltb,
  couch,
  onClick,
}: {
  game: Classification;
  hltb?: HltbEntry;
  couch?: CouchProfile;
  onClick: () => void;
}) {
  const [imgError, setImgError] = useState(false);
  const color = CATEGORY_COLORS[game.category];
  const mainHours = hltb?.main_story_hours;
  const gamepad =
    couch && !couch.vrOnly && (couch.controller === "full" || couch.controller === "partial")
      ? couch.controller
      : null;

  return (
    <button
      onClick={onClick}
      className="game-card-hover group relative rounded-lg overflow-hidden bg-steam-surface border border-steam-border hover:border-steam-blue transition-all text-left"
    >
      {/* Controller support badge */}
      {gamepad && (
        <span
          className={`absolute top-1.5 right-1.5 z-10 flex items-center gap-1 px-1.5 py-0.5 rounded-md text-[10px] font-medium backdrop-blur-sm ${
            gamepad === "full"
              ? "bg-steam-blue/85 text-white"
              : "bg-black/65 text-steam-text"
          }`}
          title={
            gamepad === "full"
              ? "Full controller support"
              : "Partial controller support — may need a keyboard"
          }
        >
          <GamepadIcon size={11} />
          {gamepad === "partial" && "partial"}
        </span>
      )}

      {/* Game image */}
      <div className="aspect-[460/215] bg-steam-surface-light">
        {!imgError ? (
          <img
            src={STEAM_HEADER_URL(game.appid)}
            alt={game.name}
            className="w-full h-full object-cover"
            loading="lazy"
            onError={() => setImgError(true)}
          />
        ) : (
          <div className="w-full h-full flex items-center justify-center text-steam-text-dim text-xs p-2 text-center">
            {game.name}
          </div>
        )}
      </div>

      {/* Info */}
      <div className="p-2">
        <div className="text-sm text-white truncate font-medium">
          {game.name}
        </div>
        <div className="flex items-center justify-between mt-1">
          <div className="flex items-center gap-1.5">
            <span
              className="w-2 h-2 rounded-full shrink-0"
              style={{ backgroundColor: color }}
            />
            <span className="text-xs text-steam-text-dim truncate">
              {CATEGORY_LABELS[game.category]}
            </span>
          </div>
          {mainHours != null && (
            <span className="text-[10px] text-steam-text-dim">
              <span className="text-steam-blue font-semibold">{mainHours}h</span> main
            </span>
          )}
        </div>
      </div>
    </button>
  );
}
