#!/usr/bin/env bash
# Restore a dump the backup sidecar wrote into a database.
#
#   restore.sh FILE [DATABASE]
#
# FILE is a path inside the container (e.g. /backups/stacks-20261101T030000Z.dump).
# DATABASE defaults to PGDATABASE. Every object in the dump is dropped and
# recreated (`--clean --if-exists`) in one transaction, so a failed restore
# leaves the database as it was. Stop stacks-api first: it holds connections
# and would write while the restore runs. See docs/DEPLOY.md, "Restore".
set -euo pipefail

file="${1:?usage: restore.sh FILE [DATABASE]}"
database="${2:-${PGDATABASE:?set PGDATABASE or pass DATABASE}}"

if [ ! -f "$file" ]; then
    echo "restore: no such file: ${file}" >&2
    exit 2
fi

pg_restore --list "$file" > /dev/null
pg_restore --clean --if-exists --no-owner --single-transaction --exit-on-error \
    --dbname="$database" "$file"
echo "restore: ${file} restored into ${database}"
