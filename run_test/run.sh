#!/bin/bash
set -e

# 現在のディレクトリをスクリプトのある場所に変更
cd "$(dirname "$0")"

# 環境変数の設定
export PG_TEST_DSN="postgres://postgres_test:postgres_test@localhost:5433/postgres_test"
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
DATABASE_URL=$PG_TEST_DSN diesel migration run
cd run_test

echo "=== テスト環境の準備が完了しました ==="
echo "テスト用DB接続情報: $PG_TEST_DSN"
echo "テストを実行するには、別のターミナルで以下のコマンドを実行してください:"
echo "PG_DSN=$PG_TEST_DSN cargo test -- --nocapture"
echo ""
echo "テスト環境を停止するには、以下のコマンドを実行してください:"
echo "cd $(pwd) && docker-compose down"
