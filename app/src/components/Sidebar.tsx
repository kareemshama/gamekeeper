import { CategoryKey, CATEGORY_LABELS, CATEGORY_COLORS } from "../lib/commands";
import { GamepadIcon } from "./icons";

type FilterKey = CategoryKey | "ALL";

export type AppView = "library" | "discover" | "taste";

interface Props {
  activeView: AppView;
  onViewChange: (view: AppView) => void;
  activeCategory: FilterKey;
  onCategoryChange: (cat: FilterKey) => void;
  counts: Record<FilterKey, number>;
  /** Couch / TV filter — layers on top of the selected collection. */
  couchOn: boolean;
  onCouchToggle: () => void;
  couchCount: number;
  onResync: () => void;
  onWriteToSteam: () => void;
  onSettings: () => void;
  onChat: () => void;
}

const VIEWS: { key: AppView; label: string; icon: string }[] = [
  { key: "library", label: "Library", icon: "▤" },
  { key: "discover", label: "Discover", icon: "◈" },
  { key: "taste", label: "Taste Profile", icon: "◉" },
];

const FILTERS: { key: FilterKey; label: string; color?: string }[] = [
  { key: "ALL", label: "All Games" },
  { key: "COMPLETED", label: "Completed", color: CATEGORY_COLORS.COMPLETED },
  { key: "IN_PROGRESS", label: "In Progress", color: CATEGORY_COLORS.IN_PROGRESS },
  { key: "ENDLESS", label: "Endless", color: CATEGORY_COLORS.ENDLESS },
  { key: "NOT_A_GAME", label: "Not a Game", color: CATEGORY_COLORS.NOT_A_GAME },
];

export default function Sidebar({ activeView, onViewChange, activeCategory, onCategoryChange, counts, couchOn, onCouchToggle, couchCount, onResync, onWriteToSteam, onSettings, onChat }: Props) {
  return (
    <aside className="w-56 bg-steam-surface flex flex-col border-r border-steam-border shrink-0">
      <nav className="p-2 space-y-1 border-b border-steam-border">
        {VIEWS.map(({ key, label, icon }) => {
          const active = activeView === key;
          return (
            <button
              key={key}
              onClick={() => onViewChange(key)}
              className={`w-full flex items-center gap-2.5 px-3 py-2 rounded-lg text-sm transition-colors ${
                active
                  ? "bg-steam-surface-light text-white"
                  : "text-steam-text-dim hover:text-white hover:bg-steam-surface-light/50"
              }`}
            >
              <span className={active ? "text-steam-blue" : ""}>{icon}</span>
              {label}
            </button>
          );
        })}
      </nav>

      <div className="flex-1 overflow-y-auto">
        {activeView === "library" && (
          <>
            <div className="p-4 pb-2">
              <h2 className="text-xs font-semibold text-steam-text-dim tracking-wide uppercase">
                Collections
              </h2>
            </div>
            <nav className="p-2 pt-0 space-y-1">
              {FILTERS.map(({ key, label, color }) => {
                const active = activeCategory === key;
                return (
                  <button
                    key={key}
                    onClick={() => onCategoryChange(key)}
                    className={`w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-colors ${
                      active
                        ? "bg-steam-surface-light text-white"
                        : "text-steam-text-dim hover:text-white hover:bg-steam-surface-light/50"
                    }`}
                  >
                    <span className="flex items-center gap-2">
                      {color && (
                        <span
                          className="w-2.5 h-2.5 rounded-full shrink-0"
                          style={{ backgroundColor: color }}
                        />
                      )}
                      {label}
                    </span>
                    <span className="text-xs text-steam-text-dim">
                      {counts[key]}
                    </span>
                  </button>
                );
              })}
            </nav>

            <div className="p-4 pb-2">
              <h2 className="text-xs font-semibold text-steam-text-dim tracking-wide uppercase">
                Play style
              </h2>
            </div>
            <nav className="p-2 pt-0 space-y-1">
              <button
                onClick={onCouchToggle}
                role="switch"
                aria-checked={couchOn}
                title="Gamepad-friendly games for a TV setup — layers on top of the collection above"
                className={`w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-colors ${
                  couchOn
                    ? "bg-steam-blue/15 text-white border border-steam-blue/40"
                    : "text-steam-text-dim hover:text-white hover:bg-steam-surface-light/50 border border-transparent"
                }`}
              >
                <span className="flex items-center gap-2">
                  <GamepadIcon className={couchOn ? "text-steam-blue" : ""} />
                  Couch / TV
                </span>
                <span className="text-xs text-steam-text-dim">{couchCount}</span>
              </button>
            </nav>
          </>
        )}
      </div>

      <div className="p-3 border-t border-steam-border space-y-2">
        <button
          onClick={onWriteToSteam}
          className="w-full py-2 px-3 rounded-lg text-sm font-medium bg-steam-blue text-white hover:bg-steam-blue-hover transition-colors"
        >
          Write to Steam
        </button>
        <button
          onClick={onResync}
          className="w-full py-2 px-3 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
        >
          Re-sync library
        </button>
        <button
          onClick={onChat}
          className="w-full py-2 px-3 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
        >
          What should I play?
        </button>
        <button
          onClick={onSettings}
          className="w-full py-2 px-3 rounded-lg text-sm text-steam-text-dim hover:text-white hover:bg-steam-surface-light transition-colors"
        >
          Settings
        </button>
      </div>

    </aside>
  );
}
