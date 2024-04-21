#!/usr/bin/env bash

mkdir -pv .data
docker-compose up -d

echo 'to stop the container, run: docker-compose down'
