# doll

OpenClaw の感情をリアルタイムで映す、デスクトップマスコットアプリ。

## 概要

**doll** は、ローカルで動いている [OpenClaw](https://github.com/anthropics/openclaw) AI エージェントと連動するデスクトップマスコットです。エージェントが回答するたびに、その感情に応じてキャラクターの表情が変わります。

- 透明ウィンドウで常に最前面に表示
- ドラッグで好きな場所に配置
- OpenClaw が回答するたびに表情がリアルタイムで変化
- 一定時間通知がなければ自動でアイドル状態に戻る
- **VoiSona Talk 連携**: エージェントの応答を音声で読み上げ (オプション)
- **スキンシステム**: キャラクターや感情の種類を自由にカスタマイズ

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

初回起動時に `~/.config/doll/config.toml` とデフォルトスキンが自動作成されます。

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

これにより、エージェントが回答するたびに doll に感情と応答テキストが通知され、表情の変化と音声読み上げが連動します。

Skill の詳細は [`skills/doll/SKILL.md`](skills/doll/SKILL.md) を参照してください。

### 動作テスト

OpenClaw なしでもテストできます:

```bash
# 感情の変化を確認
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"こんにちは！"}'

# 現在のスキンで使える感情の一覧
curl http://127.0.0.1:3000/emotions
```

---

## スキンシステム

doll はキャラクターの**スキン切り替え**に対応しています。スキンごとに異なる感情セットや VoiSona Talk のボイスを設定できます。

### スキンディレクトリ構成

各スキンは `~/.config/doll/skins/` 配下のディレクトリです:

```
~/.config/doll/skins/
└── my-character/
    ├── idle.png         # 必須: アイドル状態 + フォールバック画像
    ├── happy.png        # 任意: 感情名 = ファイル名
    ├── sad.png
    ├── doya.png         # キャラ固有の感情も自由に追加可能
    ├── ...
    └── skin.toml        # 任意: メタデータ
```

- **`idle.png` が唯一の必須ファイル** — これさえあれば有効なスキン
- それ以外の `.png` はファイル名 (拡張子除去) が感情名として自動登録される
- 感情の種類はスキン作者の自由
- OpenClaw がスキンに存在しない感情を送った場合は `idle.png` にフォールバック

### skin.toml (任意)

スキンディレクトリに `skin.toml` を置くと、メタデータを定義できます:

- `display_name` — 表示名。省略時はディレクトリ名が使われる
- `[voice]` — VoiSona Talk のボイスライブラリ指定 (スキンごとに異なるボイスを使える)
- `[emotions.*]` — 各感情の説明と、VoiSona Talk のスタイルウェイト

具体的なフォーマットはバンドル済みスキン `src-tauri/resources/skins/tama/skin.toml` を参照してください。

### スキンの追加方法

1. `~/.config/doll/skins/my-character/` を作成
2. `idle.png` を配置
3. 感情 PNG を追加 (`happy.png`, `embarrassed.png` など — 名前は自由)
4. (任意) `skin.toml` でメタデータを定義
5. `~/.config/doll/config.toml` で `skin = "my-character"` に変更
6. doll を再起動

### デフォルトスキン

`tama` スキンがアプリにバンドルされており、初回起動時に `~/.config/doll/skins/tama/` へ自動コピーされます。

---

## 設定

すべての設定は `~/.config/doll/config.toml` で管理します。初回起動時に自動作成されます。アプリ内メニュー (⚙ → 設定ファイルを開く) からも編集できます。

### VoiSona Talk TTS

`config.toml` の `[voisona]` セクションを編集してください:

```toml
[voisona]
enabled = true
port = 32766
username = "your-email@example.com"
password = "your-api-password"
```

VoiSona Talk が REST API を有効にした状態で起動している必要があります。セットアップ手順は [VoiSona Talk REST API チュートリアル](https://manual.voisona.com/ja/talk/pc/2b6e9bc7efb180ea86ccc6c7347e9ca6) を参照してください。

---

## 技術スタック

| レイヤー | 技術 |
|---------|------|
| デスクトップ | Tauri 2 |
| フロントエンド | React 19 + TypeScript (Vite 7) |
| バックエンド | Rust (axum HTTP サーバー) |
| リンター | Biome (TS) / clippy + rustfmt (Rust) |

## ライセンス

MIT
