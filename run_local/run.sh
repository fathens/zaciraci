#!/usr/bin/env bash

set -e

export CARGO_BUILD_ARGS="$@"

mkdir -pv .data
docker-compose up -d --build --remove-orphans

echo 'to stop the container, run: docker-compose down'
