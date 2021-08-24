#!/usr/bin/env bash

export CONTAINER_NAME=si/skopeo
export CONTAINER_VERSION=latest
export CMD="$@"

echo ${CMD}

docker run -it ${CONTAINER_NAME}:${CONTAINER_VERSION} ${CMD}