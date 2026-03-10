# doll — 設計・リポジトリガイド

**doll** は、ローカルで動作する [OpenClaw](https://github.com/your-org/openclaw) AI エージェントの状態を可視化するデスクトップマスコットアプリです。

---

## doll の機能

1. 透明・ボーダーレス・最前面固定のウィンドウにキャラクタースプライトを表示する。
2. スプライト上のどこをクリックしてもウィンドウをドラッグ移動できる。
3. OpenClaw エージェントからローカル **HTTP エンドポイント**経由でステータス更新を受信し、感情をリアルタイムで反映する。
4. アクティブな**スキン**に基づき、現在の感情に対応するスプライト画像を切り替える。

---

## 技術スタック

| レイヤー | 技術 | 備考 |
|----------|------|------|
| デスクトップシェル | **Tauri 2** | 透明ウィンドウ、IPC、ネイティブパッケージング |
| フロントエンド | **React 19 + TypeScript** | Vite 7 開発サーバー |
| バックエンド | **Rust** (Tauri プロセス) | HTTP サーバー (axum)、イベントエミッター |
| リンター/フォーマッター | **Biome** (TS) / **rustfmt + clippy** (Rust) | `pnpm lint` / `pnpm lint:rust` |

---

## リポジトリ構成

```
src/                    # フロントエンド (React + TypeScript)
├── App.tsx             # メインコンポーネント — スプライト表示、ドラッグ、メニュー
├── App.css             # 透明ウィンドウ用スタイル
└── main.tsx            # React エントリーポイント

src-tauri/              # バックエンド (Rust / Tauri)
├── tauri.conf.json     # ウィンドウ設定: 透明、装飾なし、最前面固定
├── Cargo.toml          # Rust 依存関係
├── resources/
│   └── skins/
│       └── tama/       # バンドル済みデフォルトスキン (初回起動時に ~/.config/doll/skins/ へコピー)
│           ├── idle.png
│           ├── happy.png
│           └── ...
├── capabilities/
│   └── default.json    # Tauri 権限設定 (ドラッグ、リサイズなど)
└── src/
    ├── main.rs         # バイナリエントリーポイント
    ├── lib.rs          # コアロジック: HTTP サーバー、Tauri イベントブリッジ、スキンコマンド
    ├── config.rs       # 設定ファイル読み込み (~/.config/doll/config.toml)
    ├── skin.rs         # スキンの検出・バリデーション・画像解決
    └── voisona.rs      # VoiSona Talk REST API クライアント (TTS)

skills/
└── doll/
    └── SKILL.md        # OpenClaw 向け doll 連携スキル
```

---

## アーキテクチャ

```
OpenClaw エージェント
       │
       │  GET  http://127.0.0.1:3000/emotions → 現在のスキンで使える感情一覧
       │  POST http://127.0.0.1:3000/status
       │        -d '{"status":"responding","emotion":"happy","text":"..."}'
       ▼
  Rust バックエンド (lib.rs)
   ├─ http_server()          — axum、127.0.0.1:3000 でリッスン
   ├─ handle_status()        — POST /status → Tauri イベント発火 + TTS 起動
   ├─ handle_emotions()      — GET /emotions → スキンの感情一覧を返す
   ├─ get_skin_info()        — Tauri コマンド: スキンメタデータ + 感情一覧
   ├─ get_skin_image()       — Tauri コマンド: 感情の PNG バイナリを返す
   └─ voisona::synthesize()  — テキストを VoiSona Talk REST API に転送
       │                          │
       │  Tauri イベント:          │  HTTP: POST /api/talk/v1/speech-syntheses
       │  "openclaw-status"       ▼
       ▼                     VoiSona Talk (localhost:32766)
  React フロントエンド (App.tsx)  └─ デフォルトオーディオデバイスで再生
   ├─ listen("openclaw-status") — React ステートを更新
   ├─ invoke("get_skin_info")   — 起動時に感情一覧を読み込み
   ├─ invoke("get_skin_image")  — PNG バイナリを取得し Object URL としてキャッシュ
   └─ mascot-container          — スプライトを描画、感情変化時に画像を切り替え
```

### データフロー

1. **Rust** がアプリ起動時に `http_server` を生成 (`setup` フック内)。
2. `http_server` が `127.0.0.1:3000` にバインドし、`POST /status` と `GET /emotions` を公開。
3. **OpenClaw エージェント**が `doll` スキルの指示に従い、回答のたびに `{"status":"responding","emotion":"..."}` を POST 送信。
4. ハンドラが JSON を `OpenClawStatus` にパースし、Tauri イベント `"openclaw-status"` として発火。
5. **React** が `"openclaw-status"` イベントをリッスンし、キャッシュから対応するスキン画像を取得してスプライトを更新。

### HTTP メッセージプロトコル (OpenClaw → doll)

OpenClaw が `POST /status` で送信する JSON:

```json
{ "status": "responding", "emotion": "happy", "text": "今日はいい天気ですね！" }
```

| フィールド | 型 | 値 |
|-----------|----|----|
| `status` | string (必須) | `"responding"` |
| `emotion` | string (必須) | `GET /emotions` で返される感情のいずれか。未知の値は `idle` にフォールバック |
| `text` | string (任意) | VoiSona Talk TTS で読み上げるテキスト |

OpenClaw は `GET /emotions` で利用可能な感情を問い合わせ可能:

```
GET http://127.0.0.1:3000/emotions
→ [
    { "name": "happy", "description": "嬉しい・ポジティブな応答" },
    { "name": "sad", "description": "悲しい・残念な応答" },
    ...
  ]
```

各エントリには `name` (PNG ファイル名に対応) と `description` (`skin.toml` に定義された説明) が含まれる。`skin.toml` に説明が未定義の場合、感情名がそのまま使われる。返されるリストはユーザーのアクティブスキンに依存し、対応する PNG がある感情のみ列挙される。`idle` は doll 内部のデフォルトであるため除外。

> **アイドル遷移**: 最後のステータス更新から 10 秒間新しい更新がなければ、doll は自動的にアイドル状態に戻る。エージェントが明示的に `"idle"` を送る必要はない。
>
> **TTS**: `text` が存在し VoiSona Talk が設定済みの場合 (後述の「設定」参照)、doll はテキストを VoiSona Talk に転送して音声合成する。音声はデフォルトオーディオデバイスで再生される。VoiSona Talk が利用不可の場合はサイレントに視覚のみモードにフォールバック。

### OpenClaw 連携 (Skill)

OpenClaw との連携には `skills/doll/` に同梱された Skill を使う。シンボリックリンクでインストール:

```bash
ln -s /path/to/doll/skills/doll ~/.cursor/skills/doll
```

Skill の内容は [`skills/doll/SKILL.md`](skills/doll/SKILL.md) を参照。エージェントが回答するたびに doll へ感情通知と応答テキストが送られる。

---

## スキンシステム

doll はキャラクターの**スキン切り替え**に対応している。各スキンは `~/.config/doll/skins/` 配下のディレクトリで、感情名の PNG 画像を格納する。

### スキンディレクトリ規約

```
~/.config/doll/skins/
└── tama/
    ├── skin.toml        # 任意 (display_name + 感情の説明)
    ├── idle.png         # 必須: アイドル状態 + 未知の感情のフォールバック
    ├── happy.png        # 任意: 感情名 = ファイル名
    ├── sad.png          # 任意
    ├── doya.png         # 任意: キャラ固有の感情も自由に追加可能
    └── ...
```

**ルール:**

- `idle.png` が唯一の必須ファイル — `idle.png` があるディレクトリは有効なスキン
- それ以外の `.png` はファイル名 (拡張子除去) が感情名として自動登録される
- 感情の種類はスキン作者の自由
- OpenClaw がスキンに存在しない感情を送った場合は `idle.png` にフォールバック

### skin.toml (任意)

```toml
display_name = "たま"

[emotions]
happy = "嬉しい・ポジティブな応答"
sad = "悲しい・残念な応答"
angry = "怒り・警告・不満"
surprised = "驚き・予想外の発見"
doya = "得意げ・自慢気な表情"
```

- `display_name`: ログに表示する名前。省略時はディレクトリ名がそのまま使われる
- `[emotions]`: 各感情の説明。`GET /emotions` でエージェントに返される。省略時は感情名がそのまま説明になる

### スキンの追加方法

1. `~/.config/doll/skins/my-character/` を作成
2. `idle.png` を配置 (唯一の必須ファイル、フォールバック画像も兼ねる)
3. 感情 PNG を追加: `happy.png`, `embarrassed.png`, `doya.png` など — 名前は自由
4. (任意) `skin.toml` に `display_name = "..."` と `[emotions]` を定義
5. `~/.config/doll/config.toml` で `skin = "my-character"` に変更
6. doll を再起動

### デフォルトスキンのバンドル

`tama` スキンは Tauri リソースとしてバンドルされている (`src-tauri/resources/skins/tama/`)。初回起動時に `~/.config/doll/skins/` が空なら、バンドルスキンをそこにコピーする。

### フロントエンドへの画像配信

画像は Vite の静的インポートではなく、Tauri IPC 経由で配信:

1. フロントエンドが `invoke("get_skin_image", { emotion })` を呼ぶ → Rust がディスクから PNG を読み取り
2. Rust が `tauri::ipc::Response` で生バイトを返す (base64 のオーバーヘッドなし)
3. フロントエンドが `Blob` → `URL.createObjectURL` → `<img src>` にセット
4. 起動時に全感情の画像をプリキャッシュ。感情変化時は Object URL を切り替えるだけ

---

## 開発

```bash
# フロントエンド依存関係のインストール
pnpm install

# 開発モードで起動 (Vite と Tauri の両方が起動)
pnpm tauri dev

# リント
pnpm lint          # TypeScript (Biome)
pnpm lint:rust     # Rust (clippy)

# フォーマット
pnpm format        # TypeScript (Biome)
pnpm format:rust   # Rust (rustfmt)

# プロダクションビルド
pnpm tauri build
```

### OpenClaw なしでのテスト

HTTP エンドポイントに直接リクエストを送信:

```bash
# 感情のみ (視覚)
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy"}'

# 感情 + TTS (VoiSona Talk が起動・設定済みであること)
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"今日はいい天気ですね！"}'

# アクティブスキンで使える感情一覧を確認
curl http://127.0.0.1:3000/emotions
```

### VoiSona Talk TTS の設定

`~/.config/doll/config.toml` を編集 (初回起動時に自動作成される):

```toml
[voisona]
enabled = true
port = 32766
username = "your-email@example.com"
password = "your-api-password"
# voice_name = ""      # 空のままにすると自動選択
# voice_version = ""
```

VoiSona Talk が REST API を有効にした状態で起動している必要がある。セットアップ手順は [VoiSona Talk REST API チュートリアル](https://manual.voisona.com/ja/talk/pc/2b6e9bc7efb180ea86ccc6c7347e9ca6) を参照。

---

## 設定

ユーザー向けの設定はすべて `~/.config/doll/config.toml` で管理する。ファイルが存在しない場合、初回起動時にデフォルト値で自動作成される。アプリ内メニュー (⚙ → 設定ファイルを開く) からも開ける。

新しい設定項目を追加する際は `config.toml` に追加し、`config.rs` と `DEFAULT_TEMPLATE` も更新すること。コンパイル時定数、環境変数、別の設定ファイルをユーザー設定に使ってはならない。

```toml
skin = "tama"

[voisona]
enabled = false
port = 32766
username = ""
password = ""
```

| 設定 | 場所 | デフォルト |
|------|------|-----------|
| アクティブスキン | `config.toml` → `skin` | `"tama"` |
| VoiSona TTS | `config.toml` → `[voisona]` | 無効 |
| HTTP サーバーポート | `src-tauri/src/lib.rs` → `DEFAULT_PORT` (コンパイル時) | `3000` |
| アイドルタイムアウト | `src/App.tsx` → `IDLE_TIMEOUT_SECS` (コンパイル時) | `10` 秒 |
| ウィンドウサイズ | `src-tauri/tauri.conf.json` → `app.windows` | 400 × 600 |
| 最前面固定 | `src-tauri/tauri.conf.json` → `app.windows[0].alwaysOnTop` | `true` |

---

## コーディングルール

### 全般

- **コミット前に必ずリントとフォーマットを実行する。** 以下の 4 コマンドを使用:
  - `pnpm lint` — Biome チェック (TypeScript / CSS)
  - `pnpm format` — Biome 自動修正 + フォーマット (TypeScript / CSS)
  - `pnpm lint:rust` — clippy (Rust)
  - `pnpm format:rust` — rustfmt (Rust)

### TypeScript / React (Biome)

- Biome は**インポートの並び順**を強制する — インポートはソース名のアルファベット順に保つ。
- Biome は **a11y ルール**を強制する — インタラクティブ要素にはセマンティック HTML (`<button>`, `<fieldset>` など) を使い、`<div>` に `role` 属性を付ける方法は避ける。適切なセマンティック要素がない場合のみ `role` を使用 (例: ドラッグコンテナの `role="application"`)。
- すべての `<button>` 要素に `type="button"` を付ける (Biome は暗黙の submit ボタンを拒否する)。
- props やエフェクトで使うイベントハンドラには `useCallback` を優先する。

### Rust

- すべての公開アイテムに `///` ドキュメントコメントを付ける。
- ランタイムメッセージには `log::info!` / `log::warn!` を使う (`println!` は使わない)。
- clippy 警告をゼロに保つ — CI では警告をエラー扱いにする。
- `rustfmt` のデフォルト設定でフォーマット (カスタム `.rustfmt.toml` は使わない)。

---

## コードレビューチェックリスト

「レビュー」を依頼された場合、以下を確認する:

1. **設計**: 各モジュール/関数は単一の責務を持っているか？データフローは明確で文書化されているか？Tauri イベント ↔ React ステートの境界はクリーンか？
2. **冗長性**: 未使用のインポート、デッドコード、ボイラープレートの残骸、重複ロジックはないか？
3. **Tauri ベストプラクティス**: `capabilities/` の権限は最小限か (未使用の許可がないか)？`Cargo.toml` のフィーチャーは `tauri.conf.json` と整合しているか？スポーンには `tauri::async_runtime` を使っているか (生の `tokio::spawn` ではなく)？
4. **フロントエンドベストプラクティス**: イベントハンドラは適切に `useCallback` で包まれているか？`invoke()` の Promise は処理されているか (最低限 `.catch()`)？Biome リントをエラーゼロで通過するか？
5. **Rust ベストプラクティス**: clippy 警告ゼロか？公開アイテムに `///` ドキュメントがあるか？`log` クレートを使っているか (`println!` ではなく)？ロガーバックエンドが登録されているか？
6. **命名の一貫性**: ファイル名、CSS クラス名、Rust 構造体名、Tauri イベント名は一貫した命名体系に従っているか？

---

## 今後の方向性

- **アニメーション**: 表情間のクロスフェードやスライドトランジション
- **システムトレイ**: トレイアイコンに終了/設定メニューを追加
- **ポート設定化**: コンパイル時定数ではなく設定ファイルから読み込む
- **思考状態**: OpenClaw ゲートウェイのイベントストリームを監視し、エージェントの処理開始/終了を検知
