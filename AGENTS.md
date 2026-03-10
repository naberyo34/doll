# doll — Design & Repository Guide

This document captures the architecture and conventions of **doll**, a desktop mascot app that visualises the state of a locally-running [OpenClaw](https://github.com/your-org/openclaw) AI agent.

---

## What doll does

1. Displays a transparent, borderless, always-on-top window showing a character sprite.
2. The window is draggable — click anywhere on the sprite to move it.
3. Receives status updates from the OpenClaw agent via a local **HTTP endpoint** and mirrors the agent's emotion in real time.
4. When more sprite images are added, the displayed image will switch based on the current emotion. Until then, a text label is shown.

---

## Tech stack

| Layer | Technology | Notes |
|-------|-----------|-------|
| Desktop shell | **Tauri 2** | Transparent window, IPC, native packaging |
| Frontend | **React 19 + TypeScript** | Vite 7 dev server |
| Backend | **Rust** (Tauri process) | HTTP server (axum), event emitter |
| Linter/Formatter | **Biome** (TS) / **rustfmt + clippy** (Rust) | `pnpm lint` / `pnpm lint:rust` |

---

## Repository structure

```
src/                    # Frontend (React + TypeScript)
├── App.tsx             # Main component — sprite display, drag, status overlay, menu
├── App.css             # Transparent-window styles
├── main.tsx            # React entry point
└── assets/
    └── tama/           # Character sprite images (transparent PNGs)
        └── neutral.png # Default / idle expression

src-tauri/              # Backend (Rust / Tauri)
├── tauri.conf.json     # Window config: transparent, decorations off, always on top
├── Cargo.toml          # Rust dependencies
├── capabilities/
│   └── default.json    # Tauri permission grants (drag, resize, etc.)
└── src/
    ├── main.rs         # Binary entry point
    ├── lib.rs          # Core logic: HTTP server, Tauri event bridge, mock command
    ├── config.rs       # Configuration loading (~/.config/doll/config.toml)
    └── voisona.rs      # VoiSona Talk REST API client for TTS
```

---

## Architecture

```
OpenClaw Agent
       │
       │  exec: curl -X POST http://127.0.0.1:3000/status
       │        -d '{"status":"responding","emotion":"happy","text":"..."}'
       ▼
  Rust backend (lib.rs)
   ├─ http_server()        — axum, listens on 127.0.0.1:3000
   ├─ handle_status()      — POST /status → emits Tauri event + triggers TTS
   ├─ set_mock_status()    — debug command for testing without OpenClaw
   └─ voisona::synthesize() — forwards text to VoiSona Talk REST API
       │                          │
       │  Tauri Event:            │  HTTP: POST /api/talk/v1/speech-syntheses
       │  "openclaw-status"       ▼
       ▼                     VoiSona Talk (localhost:32766)
  React frontend (App.tsx)    └─ plays audio on default device
   ├─ listen("openclaw-status")  — updates React state
   ├─ statusToLabel()            — maps emotion → display text
   └─ mascot-container           — renders sprite + text overlay
```

### Data flow

1. **Rust** spawns `http_server` at app startup (inside `setup` hook).
2. `http_server` binds to `127.0.0.1:3000` and exposes `POST /status`.
3. The **OpenClaw agent**, instructed via `SOUL.md`, sends a `curl` POST with `{"status":"responding","emotion":"..."}` after each reply.
4. The handler parses the JSON into `OpenClawStatus` and emits it as a Tauri event `"openclaw-status"`.
5. **React** listens for `"openclaw-status"` events and updates the UI accordingly.

### HTTP message protocol (OpenClaw → doll)

OpenClaw sends JSON via `POST /status`:

```json
{ "status": "responding", "emotion": "happy", "text": "今日はいい天気ですね！" }
```

| Field | Type | Values |
|-------|------|--------|
| `status` | string (required) | `"responding"` |
| `emotion` | string (optional) | `"happy"`, `"sad"`, `"angry"`, `"surprised"`, `"neutral"` |
| `text` | string (optional) | Reply text to be spoken via VoiSona Talk TTS |

> **Idle transition**: doll automatically returns to idle after 10 seconds of inactivity (no new status updates). The agent does not need to send an explicit `"idle"` status.
>
> **TTS**: When `text` is present and VoiSona Talk is configured (see Configuration), doll forwards the text to VoiSona Talk for speech synthesis. The audio plays through the default audio device. If VoiSona Talk is unavailable, doll silently falls back to visual-only mode.

### OpenClaw 連携 (Skill)

OpenClaw との連携には `skills/doll/` に同梱された Skill を使います。シンボリックリンクでインストールします:

```bash
ln -s /path/to/doll/skills/doll ~/.cursor/skills/doll
```

Skill の内容は [`skills/doll/SKILL.md`](skills/doll/SKILL.md) を参照してください。エージェントが回答するたびに doll へ感情通知と応答テキストが送られます。

---

## Development

```bash
# Install frontend dependencies
pnpm install

# Run in dev mode (launches both Vite and Tauri)
pnpm tauri dev

# Lint
pnpm lint          # TypeScript (Biome)
pnpm lint:rust     # Rust (clippy)

# Format
pnpm format        # TypeScript (Biome)
pnpm format:rust   # Rust (rustfmt)

# Production build
pnpm tauri build
```

### Testing without OpenClaw

Click the **⚙** icon in the top-right corner of the mascot window to open the menu. From there you can send mock status events (idle, happy, sad, angry, surprised) without needing a running OpenClaw instance.

You can also test the HTTP endpoint directly:

```bash
# Emotion only (visual)
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy"}'

# Emotion + TTS (requires VoiSona Talk running and configured)
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"今日はいい天気ですね！"}'
```

### Configuring VoiSona Talk TTS

Edit `~/.config/doll/config.toml` (the file is created automatically on first launch):

```toml
[voisona]
enabled = true
port = 32766
username = "your-email@example.com"
password = "your-api-password"
# voice_name = ""      # leave empty to auto-select
# voice_version = ""
```

VoiSona Talk must be running with its REST API enabled. See the [VoiSona Talk REST API tutorial](https://manual.voisona.com/ja/talk/pc/2b6e9bc7efb180ea86ccc6c7347e9ca6) for setup instructions.

---

## Adding new expressions

1. Place a new transparent PNG in `src/assets/tama/` (e.g. `happy.png`).
2. Import it in `App.tsx` alongside `neutralImage`.
3. Create a mapping from emotion string to the imported image.
4. In the JSX, select the `src` of `<img>` based on `status.emotion` (falling back to `neutralImage`).

---

## Configuration

All user-facing configuration is managed through `~/.config/doll/config.toml`. The file is created automatically with default values on first launch if it does not exist. Users can also open it from the in-app menu (⚙ → 設定ファイルを開く).

When adding new configurable settings, add them to `config.toml` (and update `config.rs` + `DEFAULT_TEMPLATE`). Do not use compile-time constants, environment variables, or separate config files for user settings.

| Setting | Location | Default |
|---------|----------|---------|
| VoiSona TTS | `~/.config/doll/config.toml` → `[voisona]` | disabled |
| HTTP server port | `src-tauri/src/lib.rs` → `DEFAULT_PORT` (compile-time) | `3000` |
| Idle timeout | `src/App.tsx` → `IDLE_TIMEOUT_SECS` (compile-time) | `10` seconds |
| Window size | `src-tauri/tauri.conf.json` → `app.windows` | 400 × 600 |
| Always on top | `src-tauri/tauri.conf.json` → `app.windows[0].alwaysOnTop` | `true` |

---

## Coding rules

### General

- **Always run lint and format before committing.** Use the four commands below:
  - `pnpm lint` — Biome check (TypeScript / CSS)
  - `pnpm format` — Biome auto-fix + format (TypeScript / CSS)
  - `pnpm lint:rust` — clippy (Rust)
  - `pnpm format:rust` — rustfmt (Rust)

### TypeScript / React (Biome)

- Biome enforces **import sorting** — keep imports ordered alphabetically by source.
- Biome enforces **a11y rules** — interactive elements must use semantic HTML (`<button>`, `<fieldset>`, etc.) instead of `<div>` with `role` attributes. Only use `role` when no appropriate semantic element exists (e.g. `role="application"` on the drag container).
- Use `type="button"` on all `<button>` elements (Biome rejects implicit submit buttons).
- Prefer `useCallback` for event handlers passed as props or used in effects.

### Rust

- All public items should have `///` doc comments.
- Use `log::info!` / `log::warn!` (not `println!`) for runtime messages.
- Keep `clippy` warnings at zero — treat warnings as errors in CI.
- Format with `rustfmt` defaults (no custom `.rustfmt.toml`).

---

## Code review checklist

When asked to "review", check the following:

1. **Design**: Is each module/function responsible for a single concern? Are data flows clear and documented? Is the Tauri event ↔ React state boundary clean?
2. **Redundancy**: Are there unused imports, dead code, bootstrap leftovers, or duplicate logic?
3. **Tauri best practices**: Are permissions in `capabilities/` minimal (no unused grants)? Are features in `Cargo.toml` aligned with `tauri.conf.json`? Is `tauri::async_runtime` used for spawning (not raw `tokio::spawn`)?
4. **Frontend best practices**: Are event handlers wrapped in `useCallback` where appropriate? Are Promises from `invoke()` handled (`.catch()` at minimum)? Does the code pass Biome lint with zero errors?
5. **Rust best practices**: Zero clippy warnings? Public items documented with `///`? `log` crate used (not `println!`)? Logger backend registered?
6. **Naming consistency**: Do file names, CSS class names, Rust struct names, and Tauri event names follow a coherent scheme?

---

## Future directions

- **Multiple sprite images**: swap `neutral.png` for expression-specific images based on `emotion`.
- **Animation**: cross-fade or slide transitions between expressions.
- **Speech bubble**: display the latest OpenClaw response text in a floating bubble.
- **System tray**: add a tray icon with quit/settings menu.
- **Configurable port**: read from a config file or environment variable instead of a compile-time constant.
- **Thinking state**: hook into the OpenClaw gateway event stream to detect when the agent starts/stops processing.
