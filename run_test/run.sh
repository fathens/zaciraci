#!/bin/bash
set -e

# 現在のディレクトリをスクリプトのある場所に変更
cd "$(dirname "$0")"

# 環境変数の設定
export DATABASE_URL="postgres://postgres_test:postgres_test@localhost:5433/postgres_test"
export RUST_LOG=${RUST_LOG:-debug}
export RUST_LOG_FORMAT=${RUST_LOG_FORMAT:-plain}
export RUST_BACKTRACE=${RUST_BACKTRACE:-1}

# テスト環境の起動
echo "=== テスト用Postgresを起動します ==="
docker-compose up -d

# データベースが完全に起動するまで待機
echo "=== データベースの起動を待機しています ==="
for i in {1..30}; do
  if docker-compose exec postgres pg_isready -U postgres_test -d postgres_test > /dev/null 2>&1; then
    echo "=== データベースの準備ができました ==="
    break
  fi
  echo "待機中... $i 秒経過"
  sleep 1
  if [ $i -eq 30 ]; then
    echo "タイムアウト: データベースの起動に失敗しました"
    docker-compose down
    exit 1
  fi
done

# マイグレーションの実行
echo "=== マイグレーションを実行します ==="
cd ..

# Preflight gate: migrations の preflight.sql を順に実行し、違反行があれば
# migration 適用前に exit 1 で停止する。Diesel は preflight.sql を拾わない
# ため、forensic 保全（自動 cleanup を避け operator triage を強制する）と
# 自動化（CI で違反検知を確実に走らせる）を両立する目的のゲート。違反ログは
# /tmp/preflight-*.log として残し、CI artifact 等で回収できるようにする。
shopt -s nullglob
for preflight in migrations/*/preflight.sql; do
  migration_name="$(basename "$(dirname "$preflight")")"
  log="/tmp/preflight-${migration_name}.log"
  echo "=== preflight: $migration_name ==="
  if ! psql "$DATABASE_URL" --no-align --tuples-only -f "$preflight" > "$log" 2>&1; then
    echo "preflight $migration_name failed: see $log"
    cat "$log"
    exit 1
  fi
  # 実出力行（空行除外）が残っていれば違反あり
  if grep -q '[^[:space:]]' "$log"; then
    echo "preflight $migration_name reported violators (see $log):"
    cat "$log"
    echo "Resolve violators per the migration's preflight.sql triage guidance, then re-run."
    exit 1
  fi
done
shopt -u nullglob

diesel migration run
cd run_test

echo "=== テスト環境の準備が完了しました ==="
echo "テスト用DB接続情報: $DATABASE_URL"
echo "テストを実行するには、別のターミナルで以下のコマンドを実行してください:"
echo "DATABASE_URL=$DATABASE_URL cargo test -- --nocapture"
echo ""
echo "テスト環境を停止するには、以下のコマンドを実行してください:"
echo "cd $(pwd) && docker-compose down"
