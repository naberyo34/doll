---
name: doll
description: Send emotion and reply text to the doll desktop mascot after every user-visible reply. Use after generating any response to the user so doll can update its expression and speak the reply aloud via VoiSona Talk TTS.
user-invocable: false
metadata: {"openclaw":{"always":true}}
---

# Doll Integration

After every user-visible reply, send a background HTTP request to notify the doll desktop mascot of your emotion and reply text.

## Discovering the port

doll listens on `127.0.0.1` (default port **3000**). The user may configure a custom port in `~/.config/doll/config.toml` (respects `$XDG_CONFIG_HOME`):

```toml
port = 3000
```

At the start of a session, read the config file and look for the top-level `port` value. If the file does not exist or the key is absent, use **3000**.

## Available emotions

Before choosing an emotion, query the doll endpoint to get the list of emotions supported by the currently active skin (replace `{port}` with the discovered port):

```
GET http://127.0.0.1:{port}/emotions
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

`POST http://127.0.0.1:{port}/status`

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
- `text` は基本的に応答の本文をそのまま送る。ただしコードブロック、テーブル、長いリストなど音声読み上げに不向きな部分は省略してよい。内容を要約・言い換えする必要はない。
- `text` 中の英語の固有名詞・技術用語はカタカナ読みに変換する (例: Cursor → カーソル、Docker → ドッカー、TypeScript → タイプスクリプト)。TTS エンジンは英単語をアルファベット読みしてしまうため。
- If any request fails (connection refused, timeout, etc.), ignore the error silently. doll may not be running.

## Example

```bash
# Assuming default port 3000
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"ファイルの修正が完了しました！"}'
```
