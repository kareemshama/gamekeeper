<p align="center">
  <img src="app/src-tauri/icons/128x128@2x.png" width="128" alt="Gamekeeper icon" />
</p>

<h1 align="center">Gamekeeper</h1>

<p align="center">
  <strong>Got 500+ Steam games and no idea what to play?</strong><br/>
  Gamekeeper organizes your library, shows how long each game takes, and recommends what to play next.
</p>

<p align="center">
  <a href="https://github.com/LordVelm/gamekeeper/releases/latest"><img src="https://img.shields.io/github/v/release/LordVelm/gamekeeper?style=flat-square&color=66c0f4" alt="Latest Release" /></a>
  <a href="https://github.com/LordVelm/gamekeeper/blob/master/LICENSE"><img src="https://img.shields.io/github/license/LordVelm/gamekeeper?style=flat-square" alt="License" /></a>
  <img src="https://img.shields.io/badge/platform-Windows-0078D6?style=flat-square" alt="Windows" />
</p>

---

<p align="center">
  <img src="docs/screenshots/library.png" width="720" alt="Game library with 571 games" />
</p>
<p align="center">
  <img src="docs/screenshots/grid-short-games.png" width="720" alt="Short games filter with HLTB completion times" />
</p>
<p align="center">
  <img src="docs/screenshots/ai-chat.png" width="720" alt="AI chat recommending games based on available time" />
</p>

## What It Does

Gamekeeper automatically sorts every game in your Steam library into four collections:

| Collection | What goes here |
|---|---|
| **Completed** | Games you've finished the main story |
| **In Progress** | Completable games you haven't finished yet |
| **Endless** | Multiplayer, sandbox, strategy... games with no real ending |
| **Not a Game** | Demos, tools, soundtracks, servers |

Collections sync across machines via Steam Cloud. No manual sorting required.

**How?** 14 deterministic rules analyze your Steam data (store info, achievements, playtime). No paid APIs, no black boxes. Just your free Steam Web API key.

## Features

**Organize**
- Automatic classification with 14 priority rules
- Manual overrides when you disagree (overrides always win)
- Results persist between runs. Only new games get re-classified
- **Couch / TV filter** — narrow the library to gamepad-friendly games, optionally split-screen only, and write them to Steam as their own collection for Big Picture mode

**Discover** *(new in v4.0 — works fully offline, no AI model needed)*
- **Taste Engine** — learns what you actually like from your playtime, completions, and habits
- **Discover feed** — ~60,000 Steam games you *don't* own, ranked against your taste in milliseconds
- **Taste Profile** — "what your library says about you": your tag signature, defining games, and the kinds of games you tend to bounce off
- **Anti-recommendations** — "you've bounced off 4 competitive FPS games — this looks like one"
- **Wishlist scoring** — your Steam wishlist ranked by how likely you are to actually play each game
- **"More like this"** — similar games on every detail page, with cross-genre finds badged "unexpected"
- **HowLongToBeat integration** — Completion time estimates for every game, fetched automatically
- **"Short games" filter** — Slider to find games that fit the time you have tonight
- **"What should I play next?"** — chat that knows your library, taste, playtime, and how long each game takes

**Built right**
- Dark Steam-inspired theme
- All data stays on your machine. No cloud services, no telemetry
- Cached locally so subsequent launches are instant
- Tested on a 571-game library

## Download

**[Download the latest release](https://github.com/LordVelm/gamekeeper/releases/latest)** (Windows installer)

Or build from source:

```bash
cd app
npm install
npx tauri build
```

You'll need a free [Steam Web API key](https://steamcommunity.com/dev/apikey) and your Steam profile's game details set to **Public**.

## Optional: Local AI

Everything above works with **no AI model at all** — recommendations, Discover, and wishlist scoring are powered by the offline Taste Engine. An optional local LLM adds conversational chat and a written taste profile. Pick a tier that fits your hardware (Gamekeeper detects your VRAM and recommends one):

| Tier | Model | Download | Runs on |
|---|---|---|---|
| Standard | Qwen3.5 4B | 2.7 GB | 4 GB VRAM, or CPU with 12 GB RAM |
| Plus | Qwen3 8B | 5 GB | 8 GB VRAM |
| Max | Qwen2.5 14B | 15.7 GB | 20+ GB VRAM |

No external software, no API keys, no subscriptions. The AI is never used for core classification — rules stay canonical.

**To set up:** Settings > AI Assistant > pick a tier

## System Requirements

| | Minimum (Taste Engine, no AI chat) | With optional AI chat |
|---|---|---|
| OS | Windows 10 64-bit | Windows 10/11 64-bit |
| RAM | 4 GB | 12 GB+ |
| Storage | 200 MB | 3–16 GB free (by model tier) |
| GPU | Not required | 4 GB+ VRAM by tier (CPU works for the smallest) |

## Good to Know

- **First sync takes 10-15 minutes** for large libraries (500+ games) due to Steam API rate limits. After that, launches use cached data and only fetch new games.
- **Completion times** are from [HowLongToBeat](https://howlongtobeat.com/) (community-sourced, unofficial). Cached locally and refreshed on each sync.
- **Steam must be closed** when writing collections.
- **All data is local** — API keys in `%APPDATA%/Gamekeeper/config/settings.json`, game data in `%APPDATA%/Gamekeeper/cache/`.

## Built With

[Tauri v2](https://tauri.app/) + [React 19](https://react.dev/) + TypeScript + Rust

See [`CLAUDE.md`](./CLAUDE.md) for architecture details.

## License

[MIT](./LICENSE) — Created by [LordVelm](https://github.com/LordVelm)

## Feedback

- **Bug reports & feature requests** — [Open an issue](https://github.com/LordVelm/gamekeeper/issues)
- **Support the project** — [Buy Me a Coffee](https://buymeacoffee.com/lordvelm)
