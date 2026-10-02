import { useState, useEffect } from "react";
import {
  Classification,
  CategoryKey,
  CATEGORY_LABELS,
  CATEGORY_COLORS,
  STEAM_HEADER_URL,
  STEAM_CAPSULE_URL,
  HltbEntry,
  getAmbiguitySuggestion,
  checkAiSetup,
  AmbiguityResponse,
  SimilarGame,
  TasteFit,
  CouchProfile,
  CONTROLLER_LABELS,
  getSimilarGames,
  getGameTasteFit,
} from "../lib/commands";
import { open } from "@tauri-apps/plugin-shell";
import { GamepadIcon } from "./icons";

interface Props {
  game: Classification;
  hltb?: HltbEntry;
  couch?: CouchProfile;
  playtimeHours?: number;
  onClose: () => void;
  onOverride: (appid: number, category: CategoryKey) => void;
}

const CATEGORIES: CategoryKey[] = [
  "COMPLETED",
  "IN_PROGRESS",
  "ENDLESS",
  "NOT_A_GAME",
];

export default function GameDetail({ game, hltb, couch, playtimeHours = 0, onClose, onOverride }: Props) {
  const [aiSuggestion, setAiSuggestion] = useState<AmbiguityResponse | null>(
    null
  );
  const [aiLoading, setAiLoading] = useState(false);
  const [aiReady, setAiReady] = useState<boolean | null>(null);
  const [tasteFit, setTasteFit] = useState<TasteFit | null>(null);
  const [similar, setSimilar] = useState<SimilarGame[]>([]);

  useEffect(() => {
    checkAiSetup().then((s) => setAiReady(s.modelReady && s.serverReady)).catch(() => setAiReady(false));
    // Reset immediately so a previous game's taste data never shows under this
    // game's header, and guard against out-of-order responses.
    setTasteFit(null);
    setSimilar([]);
    let cancelled = false;
    // Both silently unavailable when the game isn't in the catalog
    getGameTasteFit(game.appid)
      .then((fit) => { if (!cancelled) setTasteFit(fit); })
      .catch(() => {});
    getSimilarGames(game.appid, 8)
      .then((sims) => { if (!cancelled) setSimilar(sims); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [game.appid]);

  async function handleAskAI() {
    setAiLoading(true);
    try {
      const result = await getAmbiguitySuggestion(game.appid);
      setAiSuggestion(result);
    } catch (e) {
      setAiSuggestion({
        suggested_category: game.category,
        rationale: `Error: ${e}`,
        used_llm: false,
      });
    }
    setAiLoading(false);
  }

  return (
    <div
      className="fixed inset-0 top-9 bg-black/60 flex items-center justify-center z-50 animate-fadeIn"
      onClick={onClose}
    >
      <div
        className="bg-steam-surface rounded-xl w-full max-w-lg mx-4 overflow-hidden border border-steam-border animate-scaleIn"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header image */}
        <img
          src={STEAM_HEADER_URL(game.appid)}
          alt={game.name}
          className="w-full aspect-[460/215] object-cover"
        />

        <div className="p-5">
          <h2 className="text-xl font-bold text-white mb-1">{game.name}</h2>

          {/* Current category */}
          <div className="flex items-center gap-2 mb-4">
            <span
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: CATEGORY_COLORS[game.category] }}
            />
            <span className="text-sm text-steam-text">
              {CATEGORY_LABELS[game.category]}
            </span>
            <span className="text-xs text-steam-text-dim px-2 py-0.5 rounded bg-steam-surface-light">
              {game.confidence}
            </span>
          </div>

          {/* Couch & TV readiness */}
          {couch && couch.controller !== "unknown" && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg">
              <div className="flex items-center gap-2">
                <GamepadIcon
                  size={13}
                  className={couch.tvReady ? "text-steam-blue" : "text-steam-text-dim"}
                />
                <span className="text-xs font-semibold text-white">
                  {CONTROLLER_LABELS[couch.controller]}
                </span>
              </div>
              {(couch.splitScreen || couch.remotePlayTv || couch.remotePlayTogether || couch.vrOnly) && (
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {couch.splitScreen && <CouchChip label="Split screen" />}
                  {couch.remotePlayTv && <CouchChip label="Remote Play on TV" />}
                  {couch.remotePlayTogether && <CouchChip label="Remote Play Together" />}
                  {couch.vrOnly && <CouchChip label="VR only" />}
                </div>
              )}
              <div className="mt-2 text-[10px] text-steam-text-dim">
                {couch.vrOnly
                  ? "Needs a VR headset, so it never lands in the Couch / TV list."
                  : couch.controller === "full"
                  ? "Menus included — playable start to finish on a gamepad."
                  : couch.controller === "partial"
                  ? "Gamepad works in-game, but expect to reach for a keyboard."
                  : "Steam lists no gamepad support for this one."}
              </div>
            </div>
          )}

          {/* HLTB completion times */}
          {hltb && hltb.match_status === "matched" && hltb.main_story_hours != null && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg">
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">~Time Left</div>
                  <div className="text-sm font-semibold text-white">
                    {(() => {
                      const remaining = Math.round((hltb.main_story_hours! - playtimeHours) * 10) / 10;
                      return remaining > 0
                        ? `${remaining}h`
                        : <span className="text-[#4CAF50]">Likely complete</span>;
                    })()}
                  </div>
                </div>
                <div>
                  <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Main Story</div>
                  <div className="text-xs text-steam-text-dim">{hltb.main_story_hours}h</div>
                </div>
                {hltb.main_extra_hours != null && (
                  <div>
                    <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Main + Extra</div>
                    <div className="text-xs text-steam-text-dim">{hltb.main_extra_hours}h</div>
                  </div>
                )}
                {hltb.completionist_hours != null && (
                  <div>
                    <div className="text-[10px] text-steam-text-dim uppercase tracking-wide">Completionist</div>
                    <div className="text-xs text-steam-text-dim">{hltb.completionist_hours}h</div>
                  </div>
                )}
              </div>
              <div className="mt-2 text-[10px] text-steam-text-dim">
                Data from{" "}
                <a
                  href="https://howlongtobeat.com"
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-steam-blue hover:underline"
                >
                  HowLongToBeat
                </a>
                {" "}&middot; community-sourced estimates
              </div>
            </div>
          )}
          {hltb && hltb.match_status === "no_match" && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg text-xs text-steam-text-dim">
              No completion time estimate available.{" "}
              <a
                href={`https://howlongtobeat.com/?q=${encodeURIComponent(game.name)}`}
                target="_blank"
                rel="noopener noreferrer"
                className="text-steam-blue hover:underline"
              >
                Search HLTB
              </a>
            </div>
          )}

          {/* Reason */}
          <div className="mb-4 p-3 rounded-lg bg-steam-bg text-sm text-steam-text-dim">
            {game.reason}
          </div>

          {/* Taste fit */}
          {tasteFit && (
            <div className="mb-4 p-3 rounded-lg bg-steam-bg text-sm">
              <div className="flex items-center gap-2">
                <span className="text-steam-blue font-mono text-xs">
                  {Math.round(tasteFit.fitScore * 100)}% taste match
                </span>
                {tasteFit.matchedTags.length > 0 && (
                  <span className="text-xs text-steam-text-dim truncate">
                    {tasteFit.matchedTags.join(" · ")}
                  </span>
                )}
              </div>
              {tasteFit.nearestAnchors.length > 0 && (
                <p className="text-xs text-steam-text-dim mt-1">
                  Closest to {tasteFit.nearestAnchors.join(" and ")} in your library
                </p>
              )}
              {tasteFit.warning && (
                <p className="text-xs text-amber-400 mt-1">⚠ {tasteFit.warning}</p>
              )}
            </div>
          )}

          {/* More like this */}
          {similar.length > 0 && (
            <div className="mb-4">
              <div className="text-[10px] text-steam-text-dim uppercase tracking-wide mb-2">
                More like this
              </div>
              <div className="flex gap-2 overflow-x-auto pb-1">
                {similar.map((s) => (
                  <button
                    key={s.appid}
                    onClick={() => open(`https://store.steampowered.com/app/${s.appid}`)}
                    className="shrink-0 w-32 text-left group"
                    title={`${s.name} — ${s.reviewPositivePct}% positive`}
                  >
                    <div className="relative">
                      <img
                        src={STEAM_CAPSULE_URL(s.appid)}
                        alt={s.name}
                        loading="lazy"
                        className="w-32 aspect-[184/69] object-cover rounded bg-steam-surface-light"
                        onError={(e) => ((e.target as HTMLImageElement).style.opacity = "0.2")}
                      />
                      {s.owned && (
                        <span className="absolute top-1 right-1 px-1 rounded text-[9px] bg-black/70 text-steam-text">
                          owned
                        </span>
                      )}
                      {s.nonObvious && !s.owned && (
                        <span className="absolute top-1 right-1 px-1 rounded text-[9px] bg-steam-blue/80 text-white">
                          unexpected
                        </span>
                      )}
                    </div>
                    <p className="text-[11px] text-steam-text-dim mt-1 truncate group-hover:text-white transition-colors">
                      {s.name}
                    </p>
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* AI suggestion */}
          {game.confidence === "MEDIUM" && !aiSuggestion && (
            aiReady === false ? (
              <div className="mb-4 p-3 rounded-lg bg-yellow-900/20 border border-yellow-700/30 text-xs text-yellow-200/80">
                AI can provide a second opinion on this classification. Download the AI model in <strong className="text-yellow-200">Settings</strong> to enable this feature.
              </div>
            ) : (
              <button
                onClick={handleAskAI}
                disabled={aiLoading}
                className="mb-4 w-full py-2 px-3 rounded-lg text-sm border border-steam-blue/30 text-steam-blue hover:bg-steam-blue/10 transition-colors disabled:opacity-50"
              >
                {aiLoading ? "Asking AI..." : "Ask AI for a second opinion"}
              </button>
            )
          )}

          {aiSuggestion && (
            <div className="mb-4 p-3 rounded-lg bg-steam-blue/10 border border-steam-blue/20 text-sm">
              <div className="flex items-center gap-2 mb-1">
                <span className="text-steam-blue font-medium">
                  AI suggests: {CATEGORY_LABELS[aiSuggestion.suggested_category as CategoryKey] || aiSuggestion.suggested_category}
                </span>
                {!aiSuggestion.used_llm && (
                  <span className="text-xs text-steam-text-dim">(rules-based)</span>
                )}
              </div>
              <div className="text-xs text-steam-text-dim">
                {aiSuggestion.rationale}
              </div>
              {aiSuggestion.suggested_category !== game.category && (
                <button
                  onClick={() =>
                    onOverride(
                      game.appid,
                      aiSuggestion.suggested_category as CategoryKey
                    )
                  }
                  className="mt-2 py-1 px-3 rounded text-xs bg-steam-blue text-white hover:bg-steam-blue-hover transition-colors"
                >
                  Apply suggestion
                </button>
              )}
            </div>
          )}

          {/* Override buttons */}
          <div className="mb-4">
            <div className="text-xs text-steam-text-dim mb-2 uppercase tracking-wide font-medium">
              Change category
            </div>
            <div className="grid grid-cols-2 gap-2">
              {CATEGORIES.map((cat) => (
                <button
                  key={cat}
                  onClick={() => onOverride(game.appid, cat)}
                  disabled={cat === game.category}
                  className="py-2 px-3 rounded-lg text-sm font-medium transition-colors border disabled:opacity-30"
                  style={{
                    borderColor: CATEGORY_COLORS[cat],
                    color:
                      cat === game.category
                        ? CATEGORY_COLORS[cat]
                        : "#c7d5e0",
                    backgroundColor:
                      cat === game.category
                        ? `${CATEGORY_COLORS[cat]}20`
                        : "transparent",
                  }}
                >
                  {CATEGORY_LABELS[cat]}
                </button>
              ))}
            </div>
          </div>

          <button
            onClick={onClose}
            className="w-full py-2 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

function CouchChip({ label }: { label: string }) {
  return (
    <span className="px-1.5 py-0.5 rounded bg-steam-surface-light text-[10px] text-steam-text">
      {label}
    </span>
  );
}
