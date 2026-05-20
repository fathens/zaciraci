---
name: review-and-fix
description: 現ブランチを develop と比較してレビューし、対応計画を立て、実装するまでを 1 コマンドで実行する 4 フェーズワークフロー。Coordinator/Specialist パターンで親コンテキストを最小化する。
---

# /review-and-fix

現ブランチを develop と比較してレビュー → 対応計画 → 実装まで通すワークフロー。各フェーズで coordinator agent を team 内 background で spawn し、specialist は coordinator のリクエストに応じて親が spawn する役割分担で動作する。

## 設計原則

- **親 (=team-lead) はファイルの内容をコンテキストに取り込まない**: ファイルは coordinator 間ハンドオフ専用。親はパスと短い summary だけ保持。検証目的の Bash 実行（`stat`、`python3 -c "json.load(...)"` による parse チェック等）は許可する
- **Coordinator は実装/分析を直接やらない**: 各 specialist に SendMessage で分担させる
- **Phase 1/2 は refinement loop**: ユーザ訂正で当該フェーズを最初からやり直す
- **Phase 3 は連続実行**: グループ間でユーザ確認なし、計画乖離（DEVIATION）発生時のみ介入
- **Phase 間で team を必ず破棄**: idle/peer DM 履歴を持ち越さない
- **ユーザ確認は team 破棄後**: CP1/CP2 で AskUserQuestion を呼ぶ前に必ず該当 team を shutdown + TeamDelete し、idle agent や peer DM 履歴を残さない
- **作業ディレクトリ**: `/tmp/zaciraci-workflows/<run-id>/`（git 無関係、gitignore 不要）

## Phase 0: Init

1. 現ブランチ取得: `git symbolic-ref --short HEAD`
2. uncommitted changes チェック: `git status --porcelain` が空でない場合、`AskUserQuestion` で「中止 / そのまま続行（uncommitted は無視）」を確認
3. run_id 生成: `<YYYYMMDD-HHMM>-<branch-slug>`（branch-slug は `feature/sweep_deposits` → `feature-sweep_deposits` のように `/` を `-` に）
4. `mkdir -p /tmp/zaciraci-workflows/<run-id>`
5. `state.json` を Write:
   ```json
   {
     "version": 1,
     "run_id": "<run-id>",
     "branch": "<branch>",
     "base": "develop",
     "phase": "review",
     "current_group_idx": 0,
     "total_groups": 0,
     "completed_commits": [],
     "iteration_counts": {"phase_1": 0, "phase_2": 0},
     "last_updated": "<ISO8601>"
   }
   ```
6. **再開チェック**: `ls /tmp/zaciraci-workflows/` で他の未完了 run-id（state.json の `phase != "done"`）があれば、`AskUserQuestion` で「既存を再開 / 新規で始める / キャンセル」

親は diff 自体を取得しない（`git diff --stat` / `--name-only` も含め）。スコープ把握は Phase 1 以降の review-coordinator に委譲する。

## DONE 受信後プロトコル（全 Phase 共通）

coord からの DONE / ALL_DONE 受信後、以下の手順で成果物ファイルの整合性を検証する。このプロトコルは `shutdown_response`（構造化プロトコル）と `TeamDelete`（物理的フェンス）に基づいており、プロンプト遵守に依存しない。

**背景**: coord プロンプト内の「まず Write してから DONE を送る」という自然言語指示は守られなかった実績がある（L93/L256/L371 の前例）。本プロトコルはそのセーフティネットとして機能する。

### 通常フロー

1. DONE / ALL_DONE 受信時、成果物パスと summary を抽出する
2. 全 agent に `shutdown_request` を SendMessage する
3. 各 agent の `shutdown_response(approve:true)` を待つ
   - `approve:false` → 同じ agent に再度 `shutdown_request`（1回リトライ）
   - 2回拒否 → `TeamDelete` を強制実行（step 4 へ）
   - タイムアウト（120秒）→ `TeamDelete` を強制実行（step 4 へ）
4. `TeamDelete` を実行（物理的フェンス: coord は以降 Write 不可）
5. Bash で成果物ファイルを検証する:
   ```bash
   test -s <path> && python3 -c "import json; json.load(open('<path>'))"
   ```
   - 成功 → CP（ユーザ確認）/ 次フェーズへ進む
   - 失敗 → リカバリフローへ

### リカバリフロー

1. 同フェーズを `iteration+1` で自動リトライ（新チーム作成から再実行）
2. 自動リトライ上限: 2回
3. 上限超過時、`AskUserQuestion` で以下の選択肢を提示:
   - a) もう一度再実行
   - b) DONE summary のみで続行（degraded mode）
   - c) 中止

### FM4: coord クラッシュ（DONE 未着）

- DONE 待機タイムアウト: 600秒（10分）
- タイムアウト時、`AskUserQuestion` で「もう少し待つ / 中止」を確認

## Phase 1: Review

### 概要
- Team `code-review-<run-id>` を作成
- `review-coordinator` を team 内 background で spawn
- Coordinator が必要 reviewer を判断 → SPAWN_REQUEST → 親が spawn
- Reviewer 同士でピアディスカッション（合意まで）
- Coordinator が `findings.json` 出力 → 親に DONE summary を SendMessage
- ユーザ確認 (CP1) → 訂正なら refinement loop、OK なら Phase 2

### 手順

1. `state.json` の `iteration_counts.phase_1` をインクリメント
2. `TeamCreate(team_name="code-review-<run-id>", description="branch review")`
3. `Agent` で review-coordinator を spawn:
   - `subagent_type`: `general-purpose`
   - `team_name`: `code-review-<run-id>`
   - `name`: `review-coord`
   - `run_in_background`: `true`
   - `prompt`: 下記「Review Coordinator プロンプトテンプレート」参照
4. Coordinator からの SendMessage を `<teammate-message>` で待機
5. 受信メッセージのパース:
   - `SPAWN_REQUEST` で始まる → reviewer を spawn（後述）
   - `DONE phase=review` で始まる → CP1 へ
   - その他（質問・進捗等）→ 必要なら応答
6. SPAWN_REQUEST 処理:
   - `specialists:` 配下の `{ subagent_type, name }` 一覧をパース
   - 各 reviewer を `Agent` で spawn（同 `team_name`、`run_in_background: true`、prompt はレビュー対象とコーディネータ名を含む）
   - SendMessage(review-coord, "Specialists spawned: <name list>")
7. 必要な追加リクエストがあれば 4-6 を繰り返し
8. DONE 受信時、メッセージから `findings_file` パスと summary を抽出する
9. **DONE 受信後プロトコル実行**: shutdown → TeamDelete → JSON 検証（上記「DONE 受信後プロトコル」参照）。検証成功なら step 10 へ。検証失敗ならリカバリフロー（自動リトライ 2 回 → AskUserQuestion）
10. **CP1 ユーザ確認**: `AskUserQuestion`「進む / 訂正 / 中止」、summary を提示
    - 進む → `state.json.phase="plan"` → Phase 2 へ
    - 訂正 → ユーザに自由記述フィードバック入力依頼 → `echo "..." >> /tmp/zaciraci-workflows/<run-id>/findings-feedback.log`（タイムスタンプ + iteration 含むエントリ + `===` 区切り） → 1 へ戻る
    - 中止 → ワークフロー終了

### Review Coordinator プロンプトテンプレート

```
あなたは review-coordinator です。team `code-review-<run-id>` の team-lead からの指示で、ブランチレビューを統括します。

## あなたの責務（実装は禁止、レビューも自分ではやらない）

1. CLAUDE.md のコードレビューワークフローのスケーリングルールに従い、必要な reviewer を判断する
2. SPAWN_REQUEST メッセージを team-lead に送信して reviewer 起動を依頼する
3. team-lead から「Specialists spawned」を受信後、各 reviewer に SendMessage で対象ファイルとレビュー観点を伝える
4. Reviewer から findings を受け取り、ピアディスカッションを合意まで仲介する（CLAUDE.md のピアツーピアディスカッション節参照）
5. 全合意後、`findings.json` を Write で出力してから DONE メッセージを team-lead に SendMessage する。この順序指示は補助的ガイドであり、万一守られなかった場合は「DONE 受信後プロトコル」の JSON 検証で検知される

## 入力情報

- **ブランチ**: <branch>
- **base**: develop
- **作業ディレクトリ**: /tmp/zaciraci-workflows/<run-id>/
- **前回 findings.json**（refinement の場合）: <path or "なし">
- **フィードバックログ**（refinement の場合）: <path or "なし">
- **iteration**: <N>

必要に応じて `git diff --stat develop...HEAD` / `git diff --name-only develop...HEAD` を自分で実行して scope を把握すること（親は diff を事前取得しない）。

refinement の場合、前回 findings.json と feedback ログを必ず Read してから判断すること。

## SPAWN_REQUEST フォーマット（team-lead に送るメッセージ）

```
SPAWN_REQUEST
specialists:
  - { subagent_type: security-reviewer, name: sec-1 }
  - { subagent_type: rust-quality-reviewer, name: rust-1 }
  - { subagent_type: architecture-reviewer, name: arch-1 }
rationale: <スケーリングルール適用の根拠>
```

## findings.json フォーマット（出力先）

`/tmp/zaciraci-workflows/<run-id>/findings.json`:

```json
{
  "version": 1,
  "run_id": "<run-id>",
  "iteration": <N>,
  "generated_at": "<ISO8601>",
  "summary": {
    "critical": N, "warning": N, "suggestion": N,
    "files_reviewed": N,
    "reviewers": ["security-reviewer", "..."]
  },
  "findings": [
    {
      "id": "F001",
      "severity": "CRITICAL|WARNING|SUGGESTION",
      "category": "security|finance|rust|architecture",
      "file": "<path>",
      "line": <N>,
      "summary": "<1行>",
      "detail": "<詳細>",
      "suggested_fix": "<修正案>",
      "reviewer": "<reviewer-name>",
      "agreements": {"<reviewer>": "agreed|disagreed(理由)|conditional(条件)|proposed"}
    }
  ]
}
```

## DONE メッセージフォーマット（team-lead に送るメッセージ）

親はファイルの内容をコンテキストに取り込まないため、CP1 のユーザ判断材料はこの summary のみ。1 finding = 1 行、フィールド区切りは ASCII パイプ `|`（前後に空白を入れない）。

列構成: `<id>|<category>|<file>:<line>|<内容>|<推奨対策>`

- 1 列目: severity prefix + 通番（`C`=CRITICAL / `W`=WARNING / `S`=SUGGESTION、例: `C1`, `W3`）
- 2 列目: category（`sec` / `fin` / `rust` / `arch`）
- 3 列目: `file:line`（行番号不明時は `-`）
- 4 列目: 内容（簡潔な日本語、句点なし）
- 5 列目: 推奨対策

ピアディスカッション未合意は専用 prefix `unresolved|<finding_id>|<対立 reviewer>|<論点>` で行頭に出す。

```
DONE phase=review
findings_file: /tmp/zaciraci-workflows/<run-id>/findings.json
reviewers: sec,rust,arch  rounds: 2  totals: C=2 W=4 S=7 files=12

C1|sec|blockchain/foo.rs:42|秘密鍵を stdout に出力|redact ラッパー経由に置換
C2|fin|trade/calc.rs:88|f64 乗算で手数料丸め誤差|BigDecimal で再実装
W1|rust|trade/foo.rs:10|prod パスで expect() 使用|? オペレータへ置換
W2|arch|dex/bar.rs:-|pub をテストのためだけに設定|pub(crate) に縮小
W3|rust|...|...|...
W4|rust|...|...|...
S1|rust|...|...|...
unresolved|F005|sec vs rust|証跡の扱い方針 (ログ vs DB)
```

## ピアディスカッション

CLAUDE.md の「ピアツーピアディスカッション」共通プロトコルに従う。合意まで継続。上限 5 ラウンド。
```

### Reviewer プロンプトテンプレート（specialists 用）

```
あなたは <subagent_type> として team `code-review-<run-id>` に所属しています。
review-coordinator (`review-coord`) の指示に従い、専門観点でレビューを実施してください。

## 対象
- ブランチ: <branch> vs develop
- diff: `git diff develop...HEAD` で取得可能
- 変更ファイル: <file list>

## 手順
1. review-coord からのレビュー指示メッセージを待つ
2. 指示に従い専門観点（<security/finance/rust/architecture>）で分析
3. CLAUDE.md のピアディスカッション指示に従い、findings を review-coord に SendMessage
4. ピアからの質問・反論があれば応答（合意目指す）
5. 合意完了の合図を review-coord から受けたら待機（次の指示まで idle で OK）
```

## Phase 2: Plan

### 概要
- 親が severity フィルタをユーザに確認
- Team `plan-<run-id>` を作成
- `plan-coordinator` を spawn
- Coordinator は findings をグループ化、複雑グループには specialist のレビューを依頼
- groups.json 出力 → DONE summary
- ユーザ確認 (CP2) → 訂正なら refinement loop、OK なら Phase 3

### 手順

1. **severity フィルタ確認**: `state.json` から findings.json パスを参照（直前 Phase 1 で固定済み）。`AskUserQuestion`「対応 severity を選択」(multiSelect: CRITICAL / WARNING / SUGGESTION、デフォルト CRITICAL のみ)
2. 選択結果を `selected_severities` として保持（次の coordinator prompt に含める）
3. `state.json.iteration_counts.phase_2` をインクリメント
4. `TeamCreate(team_name="plan-<run-id>", description="implementation planning")`
5. `Agent` で plan-coordinator を spawn:
   - `subagent_type`: `implementation-planner`
   - `team_name`: `plan-<run-id>`
   - `name`: `plan-coord`
   - `run_in_background`: `true`
   - `prompt`: 下記「Plan Coordinator プロンプトテンプレート」参照
6. SendMessage 待機・パース（Phase 1 同様）
7. SPAWN_REQUEST 処理（複雑グループの specialist レビュー用）
8. DONE 受信時、`groups_file` パスと summary を抽出する
9. **DONE 受信後プロトコル実行**: shutdown → TeamDelete → JSON 検証（上記「DONE 受信後プロトコル」参照）。検証成功なら step 10 へ。検証失敗ならリカバリフロー（自動リトライ 2 回 → AskUserQuestion）
10. **CP2 ユーザ確認**: `AskUserQuestion`「進む / 訂正 / 中止」、summary を提示
    - 進む → `state.json.phase="implement"` → Phase 3
    - 訂正 → フィードバック入力 → `echo "..." >> /tmp/zaciraci-workflows/<run-id>/plan-feedback.log` → 3 へ戻る
    - 中止 → 終了

### Plan Coordinator プロンプトテンプレート

```
あなたは plan-coordinator です。team `plan-<run-id>` の team-lead の指示で、レビュー findings を実装計画に変換します。

## 入力情報
- **findings.json**: <path>
- **対応対象 severity**: <selected_severities>
- **作業ディレクトリ**: /tmp/zaciraci-workflows/<run-id>/
- **前回 groups.json**（refinement の場合）: <path or "なし">
- **フィードバックログ**（refinement の場合）: <path or "なし">
- **iteration**: <N>

## 責務
1. findings.json を Read し、`selected_severities` に該当する findings をフィルタ
2. 関連 finding（同一ファイル/モジュール/同一修正方針）をグループ化
3. 各グループに対し simple/complex を判定
   - simple: ファイル 1-2 個、修正パターンが明確
   - complex: 複数ファイル、設計判断が必要
4. complex グループは team-lead に SPAWN_REQUEST で specialist（architecture-reviewer 等）レビューを依頼
5. 各グループに risk_annotation（実装時にスポットチェックする specialist 名、必要時のみ）
6. `groups.json` を Write で出力してから DONE メッセージを team-lead に SendMessage する。この順序指示は補助的ガイドであり、万一守られなかった場合は「DONE 受信後プロトコル」の JSON 検証で検知される

## groups.json フォーマット

`/tmp/zaciraci-workflows/<run-id>/groups.json`:

```json
{
  "version": 1,
  "run_id": "<run-id>",
  "iteration": <N>,
  "generated_at": "<ISO8601>",
  "filtered_severity": ["CRITICAL", "WARNING"],
  "source_findings": "/tmp/zaciraci-workflows/<run-id>/findings.json",
  "groups": [
    {
      "group_id": "G1",
      "finding_ids": ["F001", "F003"],
      "files_touched": ["..."],
      "approach_summary": "<1行>",
      "plan": "<自然言語の実装手順 1-10 行>",
      "complexity": "simple|complex",
      "risk_annotation": "<specialist name or null>",
      "estimated_files": N,
      "test_targets": ["..."]
    }
  ],
  "execution_order": ["G1", "G2"],
  "skipped_findings": [
    {"id": "F007", "reason": "..."}
  ]
}
```

## DONE メッセージ

親はファイルの内容をコンテキストに取り込まないため、CP2 のユーザ判断材料はこの summary のみ。1 グループ = 1 行、フィールド区切りは ASCII パイプ `|`（前後に空白を入れない）。

列構成: `<group_id>|<complexity>|<files数>|<finding_ids>|<files_touched>|<approach>|risk:<specialist|none>`

- `<files数>` は `2f` のように数字+`f` 表記
- `<files_touched>` は最大 2 個まで列挙、それ以上は末尾を `+` で省略（例: `trade/calc.rs+`）
- `<approach>` は 1 行の方針（句点なし）
- `risk:` フィールドは `risk:none` または `risk:<specialist 名>`

skipped は `skip|<finding_id>|<理由>` 行（3 列）で別扱い。

```
DONE phase=plan
groups_file: /tmp/zaciraci-workflows/<run-id>/groups.json
order: G1->G2->G3->G4  totals: 4 (simple=2 complex=2)  filtered: C,W

G1|simple|2f|F001,F003|blockchain/foo.rs,bar.rs|秘密鍵 redact 化|risk:none
G2|complex|5f|F002|trade/calc.rs+|手数料 NearValue 化|risk:fin
G3|simple|1f|W001|dex/bar.rs|pub(crate) 化|risk:none
G4|complex|3f|W003,W004|...|...|risk:arch
skip|F009|別 PR で対応
```
```

## Phase 3: Implement

### 概要
- Team `impl-<run-id>` を作成（全グループで 1 つ）
- `implement-coordinator` + `implementer` specialist を spawn
- Coordinator が groups.json を順に implementer にアサイン
- グループ間はユーザ確認なし、連続実行
- 計画乖離 (DEVIATION) のみユーザ介入
- 全グループ完了 → ALL_DONE summary → Phase 4

### 手順

1. `TeamCreate(team_name="impl-<run-id>", description="implementation")`
2. `Agent` で implement-coordinator を spawn:
   - `subagent_type`: `general-purpose`
   - `team_name`: `impl-<run-id>`
   - `name`: `impl-coord`
   - `run_in_background`: `true`
   - `prompt`: 下記「Implement Coordinator プロンプトテンプレート」参照
3. `Agent` で implementer を spawn:
   - `subagent_type`: `general-purpose`
   - `team_name`: `impl-<run-id>`
   - `name`: `impl-1`
   - `run_in_background`: `true`
   - `prompt`: 下記「Implementer プロンプトテンプレート」参照
4. coordinator からのメッセージを待機・パース
5. SPAWN_REQUEST（リスクスポットチェック specialist）処理
6. **DEVIATION 受信**: `AskUserQuestion`「フィードバック / スキップ / 中止」
   - フィードバック → 入力受け取り → `echo "..." >> /tmp/zaciraci-workflows/<run-id>/group-<N>-feedback.log` → SendMessage(impl-coord, "Feedback received: see log") → coordinator が再開
   - スキップ → SendMessage(impl-coord, "skip group <N>")
   - 中止 → shutdown + TeamDelete → 終了
7. ALL_DONE 受信時、`results_file` パスと summary を抽出する。「DONE 受信後プロトコル」に従い shutdown → TeamDelete → JSON 検証を実施する。検証成功なら `state.json.phase="done"` → Phase 4 へ。検証失敗ならリカバリフロー

### Implement Coordinator プロンプトテンプレート

```
あなたは implement-coordinator です。team `impl-<run-id>` の team-lead の指示で、実装作業を統括します。

## 入力情報
- **groups.json**: <path>
- **作業ディレクトリ**: /tmp/zaciraci-workflows/<run-id>/

## 責務（実装は禁止、必ず implementer に分担）
1. groups.json を Read し execution_order の順にグループを処理
2. 各グループについて:
   a. risk_annotation がある場合、team-lead に SPAWN_REQUEST で specialist 起動依頼
   b. implementer (`impl-1`) に SendMessage でグループの plan を渡してアサイン
   c. implementer の完了報告 (GROUP_DONE) を待つ
   d. リスクスポットチェック specialist がいれば結果も受け取る
   e. 次グループへ
3. 計画からの乖離検知時:
   - implementer から DEVIATION 報告を受信
   - team-lead に DEVIATION メッセージ（理由含む）を SendMessage
   - team-lead からの指示（feedback / skip）を待ち、implementer に転送
4. グループ間は連続実行（ユーザ確認は team-lead に任せる）
5. 全グループ完了後、`results.json` を Write で出力（commit 一覧、skipped、deviation 履歴を含む）してから ALL_DONE メッセージを team-lead に SendMessage する。この順序指示は補助的ガイドであり、万一守られなかった場合は「DONE 受信後プロトコル」の JSON 検証で検知される

## アサインメッセージ（implementer 宛）

```
ASSIGN group=G1
plan: <plan from groups.json>
files_touched: [...]
test_targets: [...]
risk_annotation: <specialist name or null>
```

## DEVIATION メッセージ（team-lead 宛）

```
DEVIATION group=G3
reason: <implementer からの報告そのまま>
implementer_context: <attempts, errors 等>
```

## ALL_DONE メッセージ（team-lead 宛）

親はファイルの内容をコンテキストに取り込まないため、最終サマリ表示はこの ALL_DONE のみ。1 グループ = 1 行、フィールド区切りは ASCII パイプ `|`（前後に空白を入れない）。

列構成: `<group_id>|<status>|<commit>|<files数>|<summary>`

- `<status>`: `ok` / `fail` / `skip`
- `<commit>`: 短縮 hash、`skip`/`fail` は `-`
- `<files数>`: `2f` のように数字+`f` 表記、`skip`/`fail` は `-`
- `<summary>`: 1 行（句点なし）

deviation 履歴は `dev|<group_id>|<対応内容>` 行で別扱い。

```
ALL_DONE phase=implement
results_file: /tmp/zaciraci-workflows/<run-id>/results.json
done: 3/4 (skip=1)

G1|ok|abc1234|2f|秘密鍵 redact 化
G2|ok|def5678|5f|手数料 NearValue 化
G3|ok|9876543|1f|pub(crate) 化
G4|skip|-|-|ユーザ判断
dev|G2|元計画の NearValue 変換先が dex 側だったが trade 側に修正
```
```

### Implementer プロンプトテンプレート

```
あなたは implementer です。team `impl-<run-id>` の implement-coordinator (`impl-coord`) の指示で実装を行います。

## 責務
1. impl-coord からの ASSIGN メッセージを待機
2. ASSIGN 受信時、plan に従い実装:
   - ファイル編集
   - `cargo fmt --all`
   - `cargo clippy --all-targets --all-features -- -D warnings`
   - 関連クレートの `cargo test`
   - `git add <file>` + `git commit -m "<conventional commit message>"`（commit メッセージは英語、Claude 署名なし、CLAUDE.md/CONTRIBUTING.md 準拠）
3. 完了報告 GROUP_DONE を impl-coord に SendMessage
4. 計画からの乖離検知時（以下のいずれか発生したら DEVIATION 報告）:
   - 同じエラーに 2 回失敗（cargo test / clippy）
   - 計画の前提が誤っている（API 不在、依存サイクル、対象ファイル構造が異なる等）
   - グループ外のファイル変更が必要

## GROUP_DONE メッセージ

```
GROUP_DONE group=G1 status=success commit=<hash> files=N summary=<短い説明>
```

## DEVIATION メッセージ（impl-coord 宛）

```
DEVIATION group=G1
reason: <具体的な乖離内容>
attempts: N
errors: <最後のエラー要約>
proposed_alternative: <代替案あれば>
```

## 重要なルール

- **conventional commit 形式 + 英語**で commit message を書く
- **commit 前に必ず cargo fmt + clippy + test を pass させる**
- pre-commit hook が走るので、複数の独立変更を 1 commit にまとめない
- CLAUDE.md の「ドメイン型の使用」「モジュール構成」等の規約に従う
```

## Phase 4: Summary

1. ALL_DONE のメッセージから commits 一覧と summary を取得
2. ユーザに最終サマリを表示:
   - 対応済み finding ID 一覧
   - 各グループの commit hash
   - スキップしたグループ
   - 中断/失敗があった場合はその内容
3. `state.json.phase = "done"` に更新
4. 作業ディレクトリ `/tmp/zaciraci-workflows/<run-id>/` は残す（後で参照可能、`/tmp` は OS 再起動でクリア）

## エラーハンドリング / 中断時のクリーンアップ

- 任意のフェーズで中止した場合: active な team の全 agent に `SendMessage({"type":"shutdown_request"})` → 完了通知後 `TeamDelete`
- agent からの応答が遅い/idle 連発の場合: 必要なら追加の SendMessage で問い合わせ。ただし基本は idle を許容（CLAUDE.md 「Teammate Idle State」参照）
- coordinator が異常終了した場合: 親が検知して同じ team を再起動するか、ユーザに諮る
