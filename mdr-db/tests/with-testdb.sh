#!/usr/bin/env bash
#
# Spin up an ephemeral Postgres, load the schema fixture, run the mdr-db
# integration tests against it, and tear the container down afterwards.
#
#   mdr-db/tests/with-testdb.sh                 # run all integration tests
#   mdr-db/tests/with-testdb.sh finders         # run one test target
#
# Requires: docker (with compose) and the psql client on PATH.
set -euo pipefail
cd "$(dirname "$0")"

export TEST_DATABASE_URL="postgres://mdr:mdr@localhost:55432/mdr_test"
target="${1:-finders}"

docker compose up -d --wait
trap 'docker compose down' EXIT

psql "$TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -q -f fixtures/schema.sql

cargo test -p mdr-db --test "$target"
