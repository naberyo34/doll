---
name: doll-notify
description: "Notify doll desktop mascot of thinking/responding state on every message"
metadata: { "openclaw": { "emoji": "🎭", "events": ["message:preprocessed", "message:sent"] } }
---

# doll-notify

Sends status updates to the [doll](https://github.com/naberyo34/doll) desktop mascot so it reflects agent state in real time.

## What It Does

- **`message:preprocessed`** — fires after the user's message is fully processed but before the agent starts thinking. Sends `emotion: "thinking"` so doll shows its thinking expression and speaks a random thinking phrase via TTS.
- **`message:sent`** — fires after the agent responds. Sends `emotion: "happy"` (default) so doll returns to a responding expression.

## Requirements

- doll must be running locally on `http://127.0.0.1:3000`.
- If doll is not running, errors are silently ignored.

## Installation

```bash
mkdir -p ~/.openclaw/hooks
cp -r /path/to/doll/hooks/doll-notify ~/.openclaw/hooks/doll-notify
```

Then enable in `~/.openclaw/openclaw.json`:

```json
{
  "hooks": {
    "internal": {
      "enabled": true,
      "entries": { "doll-notify": { "enabled": true } }
    }
  }
}
```

Verify with `openclaw hooks list`, then restart the Gateway.
