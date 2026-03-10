# doll — 設計・リポジトリガイド

**doll** は、ローカルで動作する [OpenClaw](https://github.com/anthropics/openclaw) AI エージェントの状態を可視化するデスクトップマスコットアプリです。

---

## doll の機能

1. 透明・ボーダーレス・最前面固定のウィンドウにキャラクタースプライトを表示する。
2. スプライト上のどこをクリックしてもウィンドウをドラッグ移動できる。
3. OpenClaw エージェントからローカル **HTTP エンドポイント**経由でステータス更新を受信し、感情をリアルタイムで反映する。
4. アクティブな**スキン**に基づき、現在の感情に対応するスプライト画像を切り替える。
5. **VoiSona Talk** 連携でエージェントの応答テキストを音声合成する (オプション)。

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
│   └── skins/          # バンドル済みデフォルトスキン (初回起動時にユーザーディレクトリへコピー)
├── capabilities/
│   └── default.json    # Tauri 権限設定
└── src/
    ├── main.rs         # バイナリエントリーポイント
    ├── lib.rs          # コアロジック: HTTP サーバー、Tauri イベントブリッジ
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
       │  GET  /emotions → 現在のスキンで使える感情一覧
       │  POST /status   → 感情 + テキストを通知
       ▼
  Rust バックエンド (lib.rs)
   ├─ HTTP サーバー (axum, 127.0.0.1:DEFAULT_PORT)
   ├─ POST /status   → Tauri イベント発火 + TTS 起動
   ├─ GET  /emotions → スキンの感情一覧を返す
   └─ voisona.rs     → VoiSona Talk REST API で音声合成
       │
       │  Tauri イベント: "openclaw-status"
       ▼
  React フロントエンド (App.tsx)
   ├─ listen("openclaw-status") → React ステートを更新
   ├─ invoke("get_skin_image")  → Tauri IPC で PNG バイナリを取得しキャッシュ
   └─ 感情変化時にキャッシュから画像を切り替え
```

### データフロー

1. **Rust** がアプリ起動時に HTTP サーバーを生成 (`setup` フック内)。
2. **OpenClaw エージェント**が `doll` スキルの指示に従い、回答のたびにステータスを POST 送信。
3. ハンドラが Tauri イベント `"openclaw-status"` を発火し、VoiSona Talk TTS を非同期で起動。
4. **React** がイベントをリッスンし、キャッシュ済みの画像に切り替えてスプライトを更新。

### HTTP プロトコル

エンドポイントの詳細 (フィールド、型、レスポンス形式) は以下を参照:

- **OpenClaw 向け**: [`skills/doll/SKILL.md`](skills/doll/SKILL.md) — エージェントが従うべき手順とルール
- **実装**: `src-tauri/src/lib.rs` — `handle_status()`, `handle_emotions()`

> **アイドル遷移**: 最後のステータス更新から一定秒数 (`App.tsx` の `IDLE_TIMEOUT_SECS`) 経過すると、doll は自動的にアイドル状態に戻る。エージェントが明示的に `"idle"` を送る必要はない。

### OpenClaw 連携 (Skill)

OpenClaw との連携には `skills/doll/` に同梱された Skill を使う。シンボリックリンクでインストール:

```bash
ln -s /path/to/doll/skills/doll ~/.openclaw/skills/doll
```

Skill の内容は [`skills/doll/SKILL.md`](skills/doll/SKILL.md) を参照。

---

## スキンシステム

doll はキャラクターの**スキン切り替え**に対応している。スキンの仕組み・ディレクトリ規約・追加方法は [README.md の「スキンシステム」セクション](README.md#スキンシステム) を参照。

内部実装の詳細は `src-tauri/src/skin.rs` を参照。

---

## 設定

ユーザー向けの設定はすべて `~/.config/doll/config.toml` で管理する。ファイルが存在しない場合、初回起動時にデフォルト値で自動作成される。アプリ内メニュー (⚙ → 設定ファイルを開く) からも開ける。

新しい設定項目を追加する際は `config.toml` に追加し、`config.rs` の `AppConfig` と `DEFAULT_TEMPLATE` も更新すること。コンパイル時定数、環境変数、別の設定ファイルをユーザー設定に使ってはならない。

デフォルト値と設定可能な項目は `src-tauri/src/config.rs` の `DEFAULT_TEMPLATE` を参照。

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
curl -X POST http://127.0.0.1:3000/status \
  -H "Content-Type: application/json" \
  -d '{"status":"responding","emotion":"happy","text":"こんにちは！"}'

curl http://127.0.0.1:3000/emotions
```

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

1. **設計**: 各モジュール/関数は単一の責務を持っているか？データフローは明確か？Tauri イベント ↔ React ステートの境界はクリーンか？
2. **冗長性**: 未使用のインポート、デッドコード、ボイラープレートの残骸、重複ロジックはないか？
3. **Tauri ベストプラクティス**: `capabilities/` の権限は最小限か？`Cargo.toml` のフィーチャーは `tauri.conf.json` と整合しているか？スポーンには `tauri::async_runtime` を使っているか？
4. **フロントエンドベストプラクティス**: イベントハンドラは適切に `useCallback` で包まれているか？`invoke()` の Promise は処理されているか？Biome リントをエラーゼロで通過するか？
5. **Rust ベストプラクティス**: clippy 警告ゼロか？公開アイテムに `///` ドキュメントがあるか？`log` クレートを使っているか？ロガーバックエンドが登録されているか？
6. **命名の一貫性**: ファイル名、CSS クラス名、Rust 構造体名、Tauri イベント名は一貫した命名体系に従っているか？
7. **ドキュメントの鮮度**: AGENTS.md や README.md に具体的な値 (フィールド名、レスポンス例など) をハードコードしていないか？ソースコードや SKILL.md への参照で代替できないか？

---

## 今後の方向性

- **アニメーション強化**: 表情間のクロスフェードやスライドトランジション
- **システムトレイ**: トレイアイコンに終了/設定メニューを追加
- **ポート設定化**: コンパイル時定数ではなく設定ファイルから読み込む
- **思考状態**: OpenClaw ゲートウェイのイベントストリームを監視し、エージェントの処理開始/終了を検知
