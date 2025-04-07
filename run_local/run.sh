#!/usr/bin/env bash

cd "$(dirname $0)"

set -e

export CARGO_BUILD_ARGS="$@"

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
docker compose --env-file .env up -d --build --remove-orphans

echo 'to stop the container, run: docker compose down'
