# doll

OpenClaw の感情をリアルタイムで映す、デスクトップマスコットアプリ。

<p align="center">
  <img src="src/assets/tama/neutral.png" alt="tama" width="280" />
</p>

## 概要

**doll** は、ローカルで動いている [OpenClaw](https://github.com/anthropics/openclaw) AI エージェントと連動するデスクトップマスコットです。エージェントが回答するたびに、その感情（嬉しい・悲しい・怒り・驚き…）に応じてキャラクターの表情が変わります。

- 透明ウィンドウで常に最前面に表示
- ドラッグで好きな場所に配置
- OpenClaw が回答するたびに表情がリアルタイムで変化
- 10 秒間通知がなければ自動でアイドル状態に戻る
- **VoiSona Talk 連携**: エージェントの応答を音声で読み上げ (オプション)

## セットアップ

### 前提

- [Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)
- [Rust](https://www.rust-lang.org/tools/install)
- [Tauri 2 の前提環境](https://v2.tauri.app/start/prerequisites/)

### インストール & 起動

```bash
pnpm install
pnpm tauri dev
```

### OpenClaw との接続 (Skill インストール)

このリポジトリには OpenClaw 向けの Skill が `skills/doll/` に同梱されています。

**1. Skill をシンボリックリンクでインストール:**

```bash
ln -s /path/to/doll/skills/doll ~/.openclaw/skills/doll
```

**2. `~/.openclaw/openclaw.json` に Skill を有効化する設定を追加:**

```json
{
  "skills": {
    "entries": {
      "doll": {
        "enabled": true
      }
    }
  }
}
```

既に他の設定がある場合は `skills.entries` に `"doll"` のエントリを追加してください。

これにより、エージェントが回答するたびに `POST http://127.0.0.1:3000/status` で感情と応答テキストを doll に通知し、表情の変化と音声読み上げが連動します。

Skill の詳細は [`skills/doll/SKILL.md`](skills/doll/SKILL.md) を参照してください。

### 動作テスト

OpenClaw なしでもテストできます:

```bash
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"こんにちは！"}'
```

またはウィンドウ右上の **⚙** メニューから Mock Status を送れます。

### VoiSona Talk TTS の設定

`~/.config/doll/config.toml` を作成してください:

```toml
[voisona]
enabled = true
port = 32766
username = "your-email@example.com"
password = "your-api-password"
```

詳細は [VoiSona Talk REST API チュートリアル](https://manual.voisona.com/ja/talk/pc/2b6e9bc7efb180ea86ccc6c7347e9ca6) を参照してください。

## 技術スタック

| レイヤー | 技術 |
|---------|------|
| デスクトップ | Tauri 2 |
| フロントエンド | React 19 + TypeScript (Vite 7) |
| バックエンド | Rust (axum HTTP サーバー) |
| リンター | Biome (TS) / clippy + rustfmt (Rust) |

## ライセンス

MIT
