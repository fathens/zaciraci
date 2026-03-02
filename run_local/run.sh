#!/usr/bin/env bash
# Usage: ./run.sh
# Note: 設定を変更したら、このスクリプトを実行して反映させる。
#       docker compose down などは不要。このスクリプトだけで再起動可能。
#       再起動後は設定が反映されている事をログや docker の情報で確認することを推奨する。

cd "$(dirname $0)"

set -e

export CARGO_BUILD_ARGS="$@"
export GIT_COMMIT_HASH=$(git rev-parse --short=7 HEAD 2>/dev/null || echo "unknown")

mkdir -pv .data

# postgres は再作成不要（環境変数変更の影響を受けないため）
docker compose --env-file .env up -d postgres

# readonly ユーザーを作成（存在しない場合のみ）
echo "Ensuring readonly user exists..."
docker compose exec -T postgres psql -U postgres -c "
DO \$\$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'readonly') THEN
    CREATE USER readonly WITH PASSWORD 'readonly';
    GRANT CONNECT ON DATABASE postgres TO readonly;
    GRANT USAGE ON SCHEMA public TO readonly;
  END IF;
END
\$\$;
-- 新規テーブルにも権限付与（毎回実行しても安全）
GRANT SELECT ON ALL TABLES IN SCHEMA public TO readonly;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT ON TABLES TO readonly;
" 2>/dev/null || echo "Note: postgres not ready yet, readonly user will be created on next run"

# zaciraci だけを確実に再作成（環境変数やコード変更を反映）
docker compose --env-file .env up -d --build --force-recreate --remove-orphans zaciraci

echo 'to stop the container, run: docker compose down'
