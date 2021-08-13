#!/usr/bin/env sh
# shellcheck shell=sh disable=SC2039

main() {
  set -eu
  if [ -n "${DEBUG:-}" ]; then set -v; fi
  if [ -n "${TRACE:-}" ]; then set -xv; fi

  if [ -z "${HONEYCOMB_TOKEN:-}" ]; then
    die "HONEYCOMB_TOKEN must be set"
  fi
  if [ -z "${HONEYCOMB_DATASET:-}" ]; then
    die "HONEYCOMB_DATASET must be set"
  fi

  set -x
  env \
    DOCKER_BUILDKIT=1 \
    BUILDKIT_PROGRESS=plain \
    COMPOSE_DOCKER_CLI_BUILD=1 \
    docker-compose build --parallel "$@"
}

die() {
  echo "" >&2
  echo "xxx $1" >&2
  echo "" >&2
  exit 1
}

main "$@" || exit 1