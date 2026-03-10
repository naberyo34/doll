---
name: doll
description: Send emotion and reply text to the doll desktop mascot after every user-visible reply. Use after generating any response to the user so doll can update its expression and speak the reply aloud via VoiSona Talk TTS.
user-invocable: false
metadata: {"openclaw":{"always":true}}
---

# Doll Integration

After every user-visible reply, send a background HTTP request to notify the doll desktop mascot of your emotion and reply text.

## Available emotions

Before choosing an emotion, query the doll endpoint to get the list of emotions supported by the currently active skin:

```
GET http://127.0.0.1:3000/emotions
```

This returns a JSON array of objects, each with `name` and `description`:

```json
[
  { "name": "happy", "description": "嬉しい・ポジティブな応答" },
  { "name": "sad", "description": "悲しい・残念な応答" },
  { "name": "angry", "description": "怒り・警告・不満" }
]
```

Use the `description` to understand the intended use of each emotion, especially for character-specific emotions like `"doya"` (得意げ) that may not be self-explanatory. The available emotions vary depending on the user's chosen character skin. If the request fails, fall back to `"happy"` as a safe default.

## Endpoint

`POST http://127.0.0.1:3000/status`

## Payload

```json
{
  "status": "responding",
  "emotion": "<emotion>",
  "text": "<reply text>"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `status` | string (required) | Always `"responding"` |
| `emotion` | string (required) | One of the emotions returned by `GET /emotions` — pick the one that best matches the tone of your reply |
| `text` | string (optional) | The reply text for TTS. Omit or leave empty to skip voice |

## Rules

- Send the request **after** your reply, in the background. Do not let it block the response.
- Pick `emotion` based on the overall tone of your reply. Use the list from `GET /emotions` to choose the best match. If no emotion fits well, any value is safe — doll will fall back to its idle expression for unrecognised emotions.
- Keep `text` concise. If the reply is long, summarise the key point in one or two sentences rather than sending the entire response.
- If any request fails (connection refused, timeout, etc.), ignore the error silently. doll may not be running.

## Example

```bash
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"ファイルの修正が完了しました！"}'
```
