import { useState, useEffect } from "react";
import {
  checkSteamRunning,
  getSteamAccounts,
  writeToSteam,
  SteamAccount,
  WriteMode,
  WriteReport,
} from "../lib/commands";

const COUCH_WRITE_KEY = "gamekeeper-write-couch";
const WRITE_MODE_KEY = "gamekeeper-write-mode";

/** Non-destructive by default — hand-sorted collections outrank inference. */
function loadWriteMode(): WriteMode {
  return localStorage.getItem(WRITE_MODE_KEY) === "replace" ? "replace" : "addNew";
}

interface Props {
  onClose: () => void;
  totalGames: number;
  /** Games with full controller support (excluding Not a Game). */
  couchCount: number;
  /** Same, counting partial controller support too. */
  couchCountWithPartial: number;
}

type WritePhase =
  | "checking"
  | "steam-running"
  | "select-account"
  | "confirm"
  | "writing"
  | "done"
  | "error";

interface CouchPref {
  include: boolean;
  includePartial: boolean;
}

/** Defaults to on for a first write — this collection is the reason the
 *  feature exists, and an unticked box is easy to miss. The user's choice is
 *  remembered from then on. */
function loadCouchPref(): CouchPref {
  try {
    const saved = JSON.parse(localStorage.getItem(COUCH_WRITE_KEY) || "{}");
    return { include: saved.include ?? true, includePartial: saved.includePartial ?? false };
  } catch {
    return { include: true, includePartial: false };
  }
}

export default function WriteToSteam({
  onClose,
  totalGames,
  couchCount,
  couchCountWithPartial,
}: Props) {
  const [phase, setPhase] = useState<WritePhase>("checking");
  const [accounts, setAccounts] = useState<SteamAccount[]>([]);
  const [accountPath, setAccountPath] = useState<string | null>(null);
  const [couchPref, setCouchPref] = useState<CouchPref>(loadCouchPref);
  const [report, setReport] = useState<WriteReport | null>(null);
  const [mode, setMode] = useState<WriteMode>(loadWriteMode);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    checkStatus();
  }, []);

  useEffect(() => {
    localStorage.setItem(COUCH_WRITE_KEY, JSON.stringify(couchPref));
  }, [couchPref]);

  useEffect(() => {
    localStorage.setItem(WRITE_MODE_KEY, mode);
  }, [mode]);

  const couchTotal = couchPref.includePartial ? couchCountWithPartial : couchCount;

  async function checkStatus() {
    try {
      const running = await checkSteamRunning();
      if (running) {
        setPhase("steam-running");
        return;
      }

      const accts = await getSteamAccounts();
      setAccounts(accts);

      if (accts.length === 0) {
        setError("No Steam userdata directory found. Is Steam installed?");
        setPhase("error");
      } else if (accts.length === 1) {
        setAccountPath(accts[0].path);
        setPhase("confirm");
      } else {
        setPhase("select-account");
      }
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  async function doWrite() {
    if (!accountPath) {
      setError("No Steam account selected.");
      setPhase("error");
      return;
    }
    setPhase("writing");
    try {
      const result = await writeToSteam(accountPath, {
        includeCouch: couchPref.include,
        couchIncludePartial: couchPref.includePartial,
        mode,
      });
      setReport(result);
      setPhase("done");
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  async function handleRetryCheck() {
    setPhase("checking");
    setError(null);
    await checkStatus();
  }

  return (
    <div
      className="fixed inset-0 top-9 bg-black/60 flex items-center justify-center z-50 animate-fadeIn"
      onClick={onClose}
    >
      <div
        className="bg-steam-surface rounded-xl w-full max-w-md mx-4 p-6 border border-steam-border animate-scaleIn"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-lg font-bold text-white mb-4">
          Write to Steam Collections
        </h2>

        {phase === "checking" && (
          <div className="flex items-center gap-3 text-steam-text-dim">
            <div className="w-5 h-5 border-2 border-steam-blue border-t-transparent rounded-full animate-spin" />
            Checking Steam status...
          </div>
        )}

        {phase === "steam-running" && (
          <div className="space-y-4">
            <div className="p-4 rounded-lg bg-red-900/20 border border-red-700/50">
              <div className="text-red-300 font-medium mb-1">
                Steam is running
              </div>
              <div className="text-sm text-red-300/70">
                Please close Steam completely before writing collections.
                <br />
                <span className="text-xs">
                  Steam tray icon → Exit Steam, or Task Manager → End Task
                </span>
              </div>
            </div>
            <div className="flex gap-2">
              <button
                onClick={handleRetryCheck}
                className="flex-1 py-2 rounded-lg bg-steam-blue text-white font-medium hover:bg-steam-blue-hover transition-colors"
              >
                Check again
              </button>
              <button
                onClick={onClose}
                className="flex-1 py-2 rounded-lg bg-steam-surface-light text-steam-text-dim hover:text-white transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {phase === "select-account" && (
          <div className="space-y-3">
            <div className="text-sm text-steam-text-dim mb-2">
              Multiple Steam accounts found. Select one:
            </div>
            {accounts.map((acct) => (
              <button
                key={acct.id}
                onClick={() => {
                  setAccountPath(acct.path);
                  setPhase("confirm");
                }}
                className="w-full py-3 px-4 rounded-lg bg-steam-surface-light text-white text-left hover:bg-steam-blue/20 transition-colors border border-steam-border"
              >
                Account: {acct.id}
              </button>
            ))}
            <button
              onClick={onClose}
              className="w-full py-2 rounded-lg text-sm text-steam-text-dim hover:text-white transition-colors"
            >
              Cancel
            </button>
          </div>
        )}

        {phase === "confirm" && (
          <div className="space-y-4">
            <div className="text-sm text-steam-text-dim">
              {totalGames} games into four Steam collections:{" "}
              <span className="text-steam-text">Completed</span>,{" "}
              <span className="text-steam-text">In Progress</span>,{" "}
              <span className="text-steam-text">Endless/Multiplayer</span>, and{" "}
              <span className="text-steam-text">Not a Game</span>.
            </div>

            <div className="space-y-2">
              {(
                [
                  {
                    key: "addNew" as WriteMode,
                    title: "Add new games only",
                    detail:
                      "Nothing is removed. A game is filed only if none of the four collections already has it, so your own sorting survives.",
                  },
                  {
                    key: "replace" as WriteMode,
                    title: "Replace with Gamekeeper's sorting",
                    detail:
                      "Each of the four collections becomes exactly what Gamekeeper computed. Manual sorting in them is lost.",
                  },
                ]
              ).map((opt) => (
                <label
                  key={opt.key}
                  className={`flex items-start gap-2 p-2.5 rounded-lg cursor-pointer border transition-colors ${
                    mode === opt.key
                      ? "bg-steam-blue/10 border-steam-blue/40"
                      : "bg-steam-bg border-steam-border hover:border-steam-text-dim"
                  }`}
                >
                  <input
                    type="radio"
                    name="write-mode"
                    checked={mode === opt.key}
                    onChange={() => setMode(opt.key)}
                    className="mt-0.5 accent-steam-blue"
                  />
                  <span>
                    <span className="text-sm text-white">{opt.title}</span>
                    <span className="block text-xs text-steam-text-dim mt-0.5">
                      {opt.detail}
                    </span>
                  </span>
                </label>
              ))}
            </div>

            <div className="text-xs text-steam-text-dim">
              Collections named anything else are never touched. Leftover{" "}
              <span className="text-steam-text">SBO:</span> collections from older
              versions are removed either way.
            </div>

            <div className="p-3 rounded-lg bg-steam-bg space-y-2">
              <label className="flex items-start gap-2 text-sm text-steam-text cursor-pointer">
                <input
                  type="checkbox"
                  checked={couchPref.include}
                  onChange={(e) =>
                    setCouchPref({ ...couchPref, include: e.target.checked })
                  }
                  className="mt-0.5 accent-steam-blue"
                />
                <span>
                  Also write{" "}
                  <span className="text-white font-medium">Controller Friendly</span>
                  <span className="block text-xs text-steam-text-dim mt-0.5">
                    {couchTotal} gamepad-ready games — handy in Big Picture mode on a TV.
                    {mode === "addNew" && " Added to whatever the collection already holds."}
                  </span>
                </span>
              </label>

              {couchPref.include && (
                <label className="flex items-center gap-2 pl-6 text-xs text-steam-text-dim cursor-pointer">
                  <input
                    type="checkbox"
                    checked={couchPref.includePartial}
                    onChange={(e) =>
                      setCouchPref({ ...couchPref, includePartial: e.target.checked })
                    }
                    className="accent-steam-blue"
                  />
                  Include partial controller support
                </label>
              )}
            </div>

            <div className="flex gap-2">
              <button
                onClick={doWrite}
                className="flex-1 py-2 rounded-lg bg-steam-blue text-white font-medium hover:bg-steam-blue-hover transition-colors"
              >
                Write collections
              </button>
              <button
                onClick={onClose}
                className="flex-1 py-2 rounded-lg bg-steam-surface-light text-steam-text-dim hover:text-white transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {phase === "writing" && (
          <div className="flex items-center gap-3 text-steam-text-dim">
            <div className="w-5 h-5 border-2 border-steam-blue border-t-transparent rounded-full animate-spin" />
            Writing {totalGames} games to Steam collections...
          </div>
        )}

        {phase === "done" && (
          <div className="space-y-4">
            <div className="p-4 rounded-lg bg-green-900/20 border border-green-700/50">
              <div className="text-green-300 font-medium mb-1">
                Collections written!
              </div>
              <div className="text-sm text-green-300/70">
                Start Steam to see your updated collections in the library sidebar.
              </div>
            </div>
            <div className="text-xs text-steam-text-dim">
              <div className="mb-1">Created/updated:</div>
              <ul className="space-y-0.5">
                {(report?.collections ?? []).map(([name, count]) => (
                  <li key={name} className="flex items-center justify-between gap-3">
                    <span className="text-steam-text">{name}</span>
                    <span>{count} game{count !== 1 ? "s" : ""}</span>
                  </li>
                ))}
              </ul>
              {(report?.removed?.length ?? 0) > 0 && (
                <div className="mt-2">
                  <div className="mb-1">Removed (renamed in this version):</div>
                  <ul className="space-y-0.5">
                    {report!.removed.map((name) => (
                      <li key={name}>{name}</li>
                    ))}
                  </ul>
                </div>
              )}
            </div>
            <button
              onClick={onClose}
              className="w-full py-2 rounded-lg bg-steam-blue text-white font-medium hover:bg-steam-blue-hover transition-colors"
            >
              Done
            </button>
          </div>
        )}

        {phase === "error" && (
          <div className="space-y-4">
            <div className="p-4 rounded-lg bg-red-900/20 border border-red-700/50">
              <div className="text-red-300 font-medium mb-1">Error</div>
              <div className="text-sm text-red-300/70">{error}</div>
            </div>
            <button
              onClick={onClose}
              className="w-full py-2 rounded-lg bg-steam-surface-light text-steam-text-dim hover:text-white transition-colors"
            >
              Close
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
