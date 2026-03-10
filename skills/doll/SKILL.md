---
name: doll
description: Send emotion and reply text to the doll desktop mascot after every user-visible reply. Use after generating any response to the user so doll can update its expression and speak the reply aloud via VoiSona Talk TTS.
user-invocable: false
metadata: {"openclaw":{"always":true}}
---

# Doll Integration

After every user-visible reply, send a background HTTP request to notify the doll desktop mascot of your emotion and reply text.

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
| `emotion` | string (required) | One of `happy`, `sad`, `angry`, `surprised`, `neutral` — pick the one that best matches the tone of your reply |
| `text` | string (optional) | The reply text for TTS. Omit or leave empty to skip voice |

## Rules

- Send the request **after** your reply, in the background. Do not let it block the response.
- Pick `emotion` based on the overall tone: use `happy` for positive/helpful replies, `sad` for unfortunate news, `angry` for warnings or frustration, `surprised` for unexpected findings, `neutral` for factual/routine answers.
- Keep `text` concise. If the reply is long, summarise the key point in one or two sentences rather than sending the entire response.
- If the request fails (connection refused, timeout, etc.), ignore the error silently. doll may not be running.

## Example

```bash
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"ファイルの修正が完了しました！"}'
```
