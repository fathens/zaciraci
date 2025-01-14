#!/usr/bin/env bash

cd "$(dirname $0)"

set -e

export CARGO_BUILD_ARGS="$@"

mkdir -pv .data
docker compose up -d --build --remove-orphans

echo 'to stop the container, run: docker compose down'
