#!/usr/bin/env bash
# Prove a backup restores: restore a dump into a scratch database and check
# that every table holds the same number of rows as the live database.
#
#   verify.sh [FILE]      FILE defaults to the newest dump in STACKS_BACKUP_DIR
#
# Runs in the backup sidecar's image (`just backup-verify` does that). The
# scratch database, stacks_restore_check, is dropped afterwards whatever
# happens. Compare against a quiet database: rows written after the dump was
# taken show up as a mismatch.
set -euo pipefail

dir="${STACKS_BACKUP_DIR:-/backups}"
live="${PGDATABASE:?set PGDATABASE}"
scratch="stacks_restore_check"

file="${1:-}"
if [ -z "$file" ]; then
    file="$(find "$dir" -maxdepth 1 -name 'stacks-*.dump' | sort | tail -n 1)"
fi
if [ -z "$file" ] || [ ! -f "$file" ]; then
    echo "verify: no dump to verify in ${dir}" >&2
    exit 2
fi

cleanup() { dropdb --if-exists --force "$scratch" > /dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup
createdb "$scratch"
bash "$(dirname "$0")/restore.sh" "$file" "$scratch"

counts() {
    psql --dbname="$1" --no-align --tuples-only --quiet --command "
        SELECT format('SELECT %L AS t, count(*) AS n FROM %I.%I', table_name, table_schema, table_name)
        FROM information_schema.tables
        WHERE table_schema = 'public' AND table_type = 'BASE TABLE'
        ORDER BY table_name" \
    | { sql="$(paste -sd '\n' | sed '2,$s/^/UNION ALL /')"; psql --dbname="$1" --no-align --tuples-only --quiet --field-separator=' ' --command "${sql} ORDER BY 1"; }
}

live_counts="$(counts "$live")"
restored_counts="$(counts "$scratch")"
if [ "$live_counts" != "$restored_counts" ]; then
    echo "verify: ${file} does not match ${live}:" >&2
    diff <(printf '%s\n' "$live_counts") <(printf '%s\n' "$restored_counts") >&2 || true
    exit 1
fi
echo "verify: ${file} restores; $(printf '%s\n' "$restored_counts" | wc -l) tables match ${live} row for row"
printf '%s\n' "$restored_counts"
