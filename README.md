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

### OpenClaw との接続

このリポジトリには OpenClaw 向けの **Skill** と **Hook** が同梱されています。

- **Skill** (`skills/doll/`) — エージェントが応答時に感情を選んで通知
- **Hook** (`hooks/doll-notify/`) — 思考状態の自動検知 + 通知漏れ防止のフォールバック

**1. doll を右クリック →「OpenClaw 連携をインストール」を選択**

以下が自動で行われます:

- `skills/doll/` と `hooks/doll-notify/` が `~/.openclaw/` にコピーされる
- `~/.openclaw/openclaw.json` に Skill / Hook のエントリが追加される (既存設定は保持)

> 既にインストール済みの場合も上書きされるので、Skill / Hook を更新した際にも同じ操作で再インストールできます。

**2. 確認 & 再起動:**

```bash
openclaw skills info doll   # ✓ Ready
openclaw hooks list          # doll-notify が ✓ ready
```

Gateway を再起動すれば反映されます。Hook が有効な場合、エージェントの思考中にキャラクターが thinking 表情に切り替わり、`skin.toml` の `thinking_phrases` からランダムに選ばれたフレーズを読み上げます。

Skill の詳細は [`skills/doll/SKILL.md`](skills/doll/SKILL.md)、Hook の詳細は [`hooks/doll-notify/HOOK.md`](hooks/doll-notify/HOOK.md) を参照してください。

### 動作テスト

OpenClaw なしでもテストできます (ポートは `config.toml` の `port` に合わせてください。デフォルトは 3000):

```bash
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"こんにちは！"}'

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
- `thinking_phrases` — 思考中に TTS で読み上げるフレーズのリスト (`string[]`)。ランダムに 1 つ選ばれる
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

すべての設定は `~/.config/doll/config.toml` で管理します。初回起動時に自動作成されます。アプリ内メニュー (右クリック → 設定ファイルを開く) からも編集できます。

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

## ライセンス

MIT
