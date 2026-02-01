#!/usr/bin/env bash
# Usage: ./run.sh
# Note: 設定を変更したら、このスクリプトを実行して反映させる。
#       docker compose down などは不要。このスクリプトだけで再起動可能。
#       再起動後は設定が反映されている事をログや docker の情報で確認することを推奨する。

cd "$(dirname $0)"

set -e

export CARGO_BUILD_ARGS="$@"
export GIT_COMMIT_HASH=$(git rev-parse --short=7 HEAD 2>/dev/null || echo "unknown")

LOCAL_IP=$(ifconfig en0 | grep "inet " | awk '{print $2}' | head -1)
export OLLAMA_HOST=$LOCAL_IP:11434
export OLLAMA_BASE_URL=http://$OLLAMA_HOST/api

ps aux | grep 'ollama serve' | grep -v grep | awk '{print $2}' | while read ollama_pid
do
    echo "Killing existing ollama server process with PID $ollama_pid"
    kill -9 $ollama_pid
done

echo "Starting ollama server..."
ollama serve &

mkdir -pv .data

# postgres は再作成不要（環境変数変更の影響を受けないため）
docker compose --env-file .env up -d postgres
# zaciraci だけを確実に再作成（環境変数やコード変更を反映）
docker compose --env-file .env up -d --build --force-recreate --remove-orphans zaciraci

echo 'to stop the container, run: docker compose down'
