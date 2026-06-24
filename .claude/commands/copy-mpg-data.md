---
name: copy-mpg-data
description: fly.io の Managed Postgres (zaciraci-mpg) から run_test DB へ simulate 用データ (token_rates / pool_info / prediction_records) を増分コピーする。pool_info は日次集約、他は生コピー。
---

# /copy-mpg-data

fly.io 本番の Managed Postgres から、ローカル `run_test` DB へ simulate 用の履歴データを **増分**コピーする手順。既にコピー済みの範囲の続き（run_test 各テーブルの最新以降）だけを取得して追記する。

## 接続情報

| 項目 | 値 |
|---|---|
| ソース cluster ID | `z7y24ody2gmogqd1` (= `zaciraci-mpg`, org `f-athens`, region nrt) |
| ソースユーザ | **`watcher`**（読み取り専用 reader ロール。必ずこれを使う） |
| ソース DB 名 | `fly-db` |
| 宛先 | `run_test` の docker postgres（user `postgres_test` / db `postgres_test`） |

- cluster ID が不明なら `fly mpg list --org f-athens` で確認。
- ソースへは `fly mpg connect z7y24ody2gmogqd1 -u watcher -d fly-db` に SQL を **stdin でパイプ**して使う（クエリ実行可）。`Proxying localhost:16380 ...` の行は stdout に出るので、データ取得時は `2>/dev/null` では消えない点に注意（→ `\copy` でファイルに落とすことで混入を回避する）。

## 対象テーブルとコピー方式

| テーブル | タイムスタンプ列 | 方式 | 理由 |
|---|---|---|---|
| `token_rates` | `timestamp` | 生コピー（15分間隔そのまま） | run_test とソースの cadence が一致 |
| `prediction_records` | `created_at` | 生コピー | 日次バッチ（created_at は 00:00:00） |
| `pool_info` | `timestamp` | **日次集約**（pool 毎・日末1行） | ソースは15分間隔で巨大（10M行/2週）。simulate は日次刻みで `read_from_db(sim_day)` を1日1回呼び「各プールの直近スナップショット」しか見ない（`crates/simulate/src/mock_client.rs` / `engine.rs`）。日末1行で backtest に必要十分。15分間隔は情報的価値ゼロ |

### 重要な設計判断（毎回守る）

- **`id` 列は必ず除外してロード**し、run_test の serial で再採番する。ソースと run_test の id 空間は別物で、含めると PK 衝突する。ロード後に `setval(MAX(id))` でシーケンスを調整。
- pool_info のユニーク制約は `(pool_id, timestamp)`。増分は新しいタイムスタンプなので衝突しない。
- pool_info の **run_test 最終日は部分日（コピー実行時刻で途切れ）になりがち**。これを正しい「日末スナップショット」に直すため、最終日を含めて削除→再投入する（下記手順参照）。

## 手順

### 0. コンテナ確認

```bash
docker compose -f run_test/docker-compose.yml exec -T postgres pg_isready
```

起動していなければ `cd run_test && ./run.sh`。

### 1. run_test の現状（増分の起点）を確認

```bash
docker compose -f run_test/docker-compose.yml exec -T postgres psql -U postgres_test postgres_test -t -A \
  -c "SELECT 'token_rates', COUNT(*), MAX(timestamp)::text FROM token_rates
      UNION ALL SELECT 'pool_info', COUNT(*), MAX(timestamp)::text FROM pool_info
      UNION ALL SELECT 'prediction_records', COUNT(*), MAX(created_at)::text FROM prediction_records"
```

ここで得た各テーブルの MAX を増分の境界に使う。以降、`$TR_MAX` / `$PI_MAX` / `$PR_MAX` と表記。

### 2. 増分件数をソースで確認（任意・サニティチェック）

```bash
printf '%s\n' "SELECT
  (SELECT COUNT(*) FROM token_rates WHERE timestamp > '$TR_MAX') AS tr,
  (SELECT COUNT(*) FROM prediction_records WHERE created_at > '$PR_MAX') AS pr,
  (SELECT COUNT(*) FROM pool_info WHERE timestamp >= date_trunc('day','$PI_MAX'::timestamp)) AS pi_raw;" \
  | fly mpg connect z7y24ody2gmogqd1 -u watcher -d fly-db 2>/dev/null
```

### 3. ダンプ（ソース → ローカル tsv）

作業ディレクトリ: `mkdir -p /tmp/zaciraci_copy`

`\copy ... TO 'localpath'` でローカルに書き出す（psql はローカルで動くのでローカルへ保存される。`Proxying` 行はファイルに混入しない）。フォーマットはデフォルトのテキスト（タブ区切り、NULL は `\N`、jsonb もそのまま）で、宛先の `COPY ... FROM STDIN` デフォルトと一致する。

**token_rates（生・大きい → バックグラウンド＋大きめ timeout）**

> ⚠️ ~126万行で **約15分**かかる。`timeout 300` だと途中で切れる（過去に 06-07 で切断した）。`timeout 1800` 程度にし、`run_in_background: true` で実行。完了後 `COPY <N>` 行と `wc -l` が一致することを必ず確認する。

```bash
TR_COLS="base_token, quote_token, rate, timestamp, decimals, rate_calc_near, swap_path"
printf '%s\n' "\\copy (SELECT $TR_COLS FROM token_rates WHERE timestamp > '$TR_MAX' ORDER BY timestamp) TO '/tmp/zaciraci_copy/tr.tsv'" \
  | timeout 1800 fly mpg connect z7y24ody2gmogqd1 -u watcher -d fly-db 2>/dev/null
```

**prediction_records（生・小さい）**

```bash
PR_COLS="token, quote_token, predicted_price, data_cutoff_time, target_time, actual_price, mape, absolute_error, evaluated_at, created_at"
printf '%s\n' "\\copy (SELECT $PR_COLS FROM prediction_records WHERE created_at > '$PR_MAX' ORDER BY created_at) TO '/tmp/zaciraci_copy/pred.tsv'" \
  | timeout 120 fly mpg connect z7y24ody2gmogqd1 -u watcher -d fly-db 2>/dev/null
```

**pool_info（日次集約 → ソース10M行をソートするので timeout 大きめ・バックグラウンド）**

`DISTINCT ON (pool_id, 日)` で各プール・各日の最終行（日末スナップショット）だけ抽出する。境界は run_test 最終日の **0時**にして、部分日だった最終日を上書きできるようにする。

```bash
PI_COLS="pool_id, pool_kind, token_account_ids, amounts, total_fee, shares_total_supply, amp, timestamp"
DAY0="$(date_trunc day of $PI_MAX)"   # 例: 2026-06-03 00:00:00
printf '%s\n' "\\copy (SELECT DISTINCT ON (pool_id, timestamp::date) $PI_COLS FROM pool_info WHERE timestamp >= '$DAY0' ORDER BY pool_id, timestamp::date, timestamp DESC) TO '/tmp/zaciraci_copy/pi.tsv'" \
  | timeout 1800 fly mpg connect z7y24ody2gmogqd1 -u watcher -d fly-db 2>/dev/null
```

各ダンプ後、`COPY <N>` の N と `wc -l <file>` が一致することを確認。バックグラウンド実行時は完了通知を待ってから確認する。

### 4. ロード（ローカル tsv → run_test）

`id` を含めない列リストで `COPY ... FROM STDIN`。

```bash
# token_rates
docker compose -f run_test/docker-compose.yml exec -T postgres \
  psql -U postgres_test postgres_test -c "COPY token_rates($TR_COLS) FROM STDIN" < /tmp/zaciraci_copy/tr.tsv

# prediction_records
docker compose -f run_test/docker-compose.yml exec -T postgres \
  psql -U postgres_test postgres_test -c "COPY prediction_records($PR_COLS) FROM STDIN" < /tmp/zaciraci_copy/pred.tsv

# pool_info: 部分日だった最終日を含めて削除してから投入
docker compose -f run_test/docker-compose.yml exec -T postgres \
  psql -U postgres_test postgres_test -c "DELETE FROM pool_info WHERE timestamp >= '$DAY0'"
docker compose -f run_test/docker-compose.yml exec -T postgres \
  psql -U postgres_test postgres_test -c "COPY pool_info($PI_COLS) FROM STDIN" < /tmp/zaciraci_copy/pi.tsv
```

### 5. シーケンス調整

```bash
for t in token_rates pool_info prediction_records; do
  docker compose -f run_test/docker-compose.yml exec -T postgres \
    psql -U postgres_test postgres_test -c "SELECT setval(pg_get_serial_sequence('$t','id'), MAX(id)) FROM $t"
done
```

### 6. 検証

```bash
docker compose -f run_test/docker-compose.yml exec -T postgres psql -U postgres_test postgres_test -t -A \
  -c "SELECT 'token_rates', COUNT(*), MAX(timestamp)::text FROM token_rates
      UNION ALL SELECT 'pool_info', COUNT(*), MAX(timestamp)::text FROM pool_info
      UNION ALL SELECT 'prediction_records', COUNT(*), MAX(created_at)::text FROM prediction_records"
```

pool_info の日次形状チェック（**過去日は全て hour 23、当日のみ部分日**になっていれば正常）:

```bash
docker compose -f run_test/docker-compose.yml exec -T postgres psql -U postgres_test postgres_test -t -A \
  -c "SELECT timestamp::date, COUNT(*), EXTRACT(HOUR FROM MIN(timestamp))||'-'||EXTRACT(HOUR FROM MAX(timestamp))
      FROM pool_info WHERE timestamp >= '$DAY0'::timestamp - INTERVAL '2 days' GROUP BY 1 ORDER BY 1"
```

### 7. 後始末

```bash
rm -rf /tmp/zaciraci_copy
```

## 注意・ハマりどころ

- **タイムアウト**: `fly mpg connect` 経由のダンプは proxy 越しで遅い（~1500行/秒）。token_rates / pool_info は必ず `timeout 1800` 級＋バックグラウンドで。`COPY <N>` が出ない／最終行のタイムスタンプが古い → 途中切断を疑う。
- **simulate 連続実行時の状態リセット**は別途必要（`portfolio_holdings` / `evaluation_periods` / `trade_transactions` を case 毎に TRUNCATE）。本コマンドは履歴データ投入のみで結果テーブルは触らない。
- 全件再構築が必要な場合は本コマンドではなく `run_test/copy_data.sh`（run_local→run_test の TRUNCATE フルコピー）を使う。
