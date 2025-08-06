#!/bin/sh
PORT=$(echo "${SERVICE_BIND_TO_ADDR:-0.0.0.0:8080}" | awk -F: '{print $NF}' | tr -d '"')
curl -fs "http://localhost:$PORT/health" || exit 1