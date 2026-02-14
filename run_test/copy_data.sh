#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# デフォルト値
FROM_FILE=""
START_DATE=""
END_DATE=""
TABLES="token_rates,pool_info"
KEEP_RESULTS=false

# --- ヘルプ ---
usage() {
  cat <<'USAGE'
使い方: ./run_test/copy_data.sh [オプション]

run_test DB にシミュレーション用データを投入します。

ソース指定:
  (なし)                    run_local の Docker コンテナからコピー（デフォルト）
  --from-file <path>        pg_dump で作成したダンプファイルからリストア

フィルタ（run_local モードのみ、token_rates のみに適用）:
  --start-date YYYY-MM-DD   token_rates の開始日
  --end-date   YYYY-MM-DD   token_rates の終了日

テーブル:
  --tables TABLE,...         コピーするテーブル（デフォルト: token_rates,pool_info）

シミュレーション結果:
  --keep-results             過去のシミュレーション結果テーブルを保持する
                             (trade_transactions, evaluation_periods, prediction_records)

ダンプファイルの作成例:
  # リモートサーバーから
  pg_dump -h remote -U user --data-only -t token_rates -t pool_info --no-owner db > dump.sql
  # SSH 経由
  ssh server "pg_dump -U postgres --data-only -t token_rates --no-owner db" > dump.sql
USAGE
}

# --- 引数パース ---
while [[ $# -gt 0 ]]; do
  case "$1" in
    --from-file)
      FROM_FILE="$2"
      shift 2
      ;;
    --start-date)
      START_DATE="$2"
      shift 2
      ;;
    --end-date)
      END_DATE="$2"
      shift 2
      ;;
    --tables)
      TABLES="$2"
      shift 2
      ;;
    --keep-results)
      KEEP_RESULTS=true
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "エラー: 不明なオプション: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

# --- バリデーション ---
validate_date() {
  local label="$1" value="$2"
  if [[ ! "$value" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
    echo "エラー: $label の日付フォーマットが不正です: $value (YYYY-MM-DD)" >&2
    exit 1
  fi
}

if [[ -n "$START_DATE" ]]; then
  validate_date "--start-date" "$START_DATE"
fi
if [[ -n "$END_DATE" ]]; then
  validate_date "--end-date" "$END_DATE"
fi

if [[ -n "$FROM_FILE" && ! -f "$FROM_FILE" ]]; then
  echo "エラー: ファイルが見つかりません: $FROM_FILE" >&2
  exit 1
fi

if [[ -n "$FROM_FILE" && ( -n "$START_DATE" || -n "$END_DATE" ) ]]; then
  echo "エラー: --from-file と --start-date/--end-date は同時に指定できません" >&2
  exit 1
fi

# --- ヘルパー関数 ---
dst_psql() {
  docker compose -f "$PROJECT_ROOT/run_test/docker-compose.yml" exec -T postgres \
    psql -U postgres_test postgres_test "$@"
}

src_psql() {
  docker compose -f "$PROJECT_ROOT/run_local/docker-compose.yml" exec -T postgres \
    psql -U readonly postgres "$@"
}

src_copy_out() {
  local table="$1"
  shift
  docker compose -f "$PROJECT_ROOT/run_local/docker-compose.yml" exec -T postgres \
    psql -U readonly postgres -c "COPY $table TO STDOUT" "$@"
}

check_container() {
  local compose_file="$1" label="$2"
  if ! docker compose -f "$compose_file" exec -T postgres pg_isready -q 2>/dev/null; then
    echo "エラー: $label の PostgreSQL コンテナが起動していません" >&2
    echo "  起動方法: docker compose -f $compose_file up -d" >&2
    exit 1
  fi
}

row_count() {
  local table="$1"
  dst_psql -t -A -c "SELECT COUNT(*) FROM $table"
}

src_row_count() {
  local table="$1"
  src_psql -t -A -c "SELECT COUNT(*) FROM $table"
}

fix_sequence() {
  local table="$1"
  dst_psql -c "SELECT setval(pg_get_serial_sequence('$table', 'id'), COALESCE(MAX(id), 1)) FROM $table" > /dev/null 2>&1 || true
}

# --- プログレス表示 ---
HAS_PV=false
if command -v pv &>/dev/null; then
  HAS_PV=true
fi

PROGRESS_PID=""

start_progress() {
  local msg="$1"
  if [[ "$HAS_PV" == true ]]; then
    return
  fi
  (
    local elapsed=0
    while true; do
      printf "\r  %s %d秒経過 ..." "$msg" "$elapsed" >&2
      sleep 2
      elapsed=$((elapsed + 2))
    done
  ) &
  PROGRESS_PID=$!
}

stop_progress() {
  if [[ -n "$PROGRESS_PID" ]]; then
    kill "$PROGRESS_PID" 2>/dev/null || true
    wait "$PROGRESS_PID" 2>/dev/null || true
    PROGRESS_PID=""
    printf "\r\033[K" >&2
  fi
}

# パイプにプログレス表示を挿入
pipe_with_progress() {
  if [[ "$HAS_PV" == true ]]; then
    pv -f -a -b
  else
    cat
  fi
}

trap 'stop_progress' EXIT

# --- コンテナ起動チェック ---
echo "=== コンテナの起動確認 ==="
check_container "$PROJECT_ROOT/run_test/docker-compose.yml" "run_test"

if [[ -z "$FROM_FILE" ]]; then
  check_container "$PROJECT_ROOT/run_local/docker-compose.yml" "run_local"
fi

echo "OK"

# --- シミュレーション結果テーブルのクリア ---
RESULT_TABLES="trade_transactions evaluation_periods prediction_records"

if [[ "$KEEP_RESULTS" == false ]]; then
  echo ""
  echo "=== シミュレーション結果テーブルのクリア ==="
  for rt in $RESULT_TABLES; do
    count=$(row_count "$rt")
    dst_psql -c "TRUNCATE $rt RESTART IDENTITY CASCADE" > /dev/null
    echo "  $rt: $count 行削除"
  done
fi

# --- テーブルコピー ---
IFS=',' read -ra TABLE_LIST <<< "$TABLES"

for table in "${TABLE_LIST[@]}"; do
  echo ""
  echo "=== $table のコピー ==="

  # ソースの行数表示（run_local モードのみ）
  if [[ -z "$FROM_FILE" ]]; then
    src_count=$(src_row_count "$table")
    echo "  ソース (run_local): $src_count 行"
  fi

  # ターゲットを TRUNCATE
  dst_before=$(row_count "$table")
  dst_psql -c "TRUNCATE $table RESTART IDENTITY CASCADE" > /dev/null
  echo "  ターゲット: $dst_before 行を削除"

  # データ転送
  echo "  転送中 ..."
  if [[ -n "$FROM_FILE" ]]; then
    # ファイルからリストア
    echo "  ファイルからリストア: $FROM_FILE"
    start_progress "リストア中"
    cat "$FROM_FILE" | pipe_with_progress | dst_psql > /dev/null
    stop_progress
  elif [[ "$table" == "token_rates" && ( -n "$START_DATE" || -n "$END_DATE" ) ]]; then
    # 日付フィルタ付き COPY
    where_parts=""
    if [[ -n "$START_DATE" ]]; then
      where_parts="timestamp >= '$START_DATE'"
    fi
    if [[ -n "$END_DATE" ]]; then
      if [[ -n "$where_parts" ]]; then
        where_parts="$where_parts AND "
      fi
      where_parts="${where_parts}timestamp < '$END_DATE'"
    fi
    where="WHERE $where_parts"

    filtered_count=$(src_psql -t -A -c "SELECT COUNT(*) FROM $table $where")
    echo "  フィルタ適用: $where ($filtered_count 行)"

    start_progress "コピー中"
    src_psql -c "COPY (SELECT * FROM $table $where) TO STDOUT" \
      | pipe_with_progress | dst_psql -c "COPY $table FROM STDIN" > /dev/null
    stop_progress
  else
    # COPY パイプ（全件コピー）
    start_progress "コピー中"
    src_copy_out "$table" \
      | pipe_with_progress | dst_psql -c "COPY $table FROM STDIN" > /dev/null
    stop_progress
  fi

  # シーケンス調整
  fix_sequence "$table"

  # ターゲットの行数確認
  dst_after=$(row_count "$table")
  echo "  ターゲット: $dst_after 行"
done

echo ""
echo "=== 完了 ==="
