#!/usr/bin/env bash
# Scheduled pg_dump of the Stacks database, for the `backup` compose profile
# (compose.backup.yaml). Runs inside the postgres:18 image, so pg_dump always
# matches the server's major version.
#
#   backup.sh          dump now, then every STACKS_BACKUP_INTERVAL_SECS
#   backup.sh once     dump once and exit (used by `just backup-verify`)
#
# Each dump is pg_dump's custom format (`pg_restore` reads it), written to a
# temporary name, checked with `pg_restore --list`, and only then renamed to
# stacks-YYYYMMDDTHHMMSSZ.dump, so a half-written file never looks like a
# backup. The newest STACKS_BACKUP_KEEP dumps are kept; older ones are removed.
#
# Connection settings come from the standard libpq variables (PGHOST, PGUSER,
# PGPASSWORD, PGDATABASE), which compose.backup.yaml sets.
set -euo pipefail

dir="${STACKS_BACKUP_DIR:-/backups}"
interval="${STACKS_BACKUP_INTERVAL_SECS:-86400}"
keep="${STACKS_BACKUP_KEEP:-14}"

if ! [[ "$interval" =~ ^[0-9]+$ ]] || [ "$interval" -lt 60 ]; then
    echo "STACKS_BACKUP_INTERVAL_SECS must be a whole number of seconds, at least 60" >&2
    exit 2
fi
if ! [[ "$keep" =~ ^[0-9]+$ ]] || [ "$keep" -lt 1 ]; then
    echo "STACKS_BACKUP_KEEP must be a whole number, at least 1" >&2
    exit 2
fi

mkdir -p "$dir"

dump() {
    local stamp tmp final
    stamp="$(date -u +%Y%m%dT%H%M%SZ)"
    tmp="${dir}/.stacks-${stamp}.dump.partial"
    final="${dir}/stacks-${stamp}.dump"
    if ! pg_dump --format=custom --no-owner --file="$tmp"; then
        rm -f "$tmp"
        echo "backup: pg_dump failed; the previous dumps are untouched" >&2
        return 1
    fi
    if ! pg_restore --list "$tmp" > /dev/null; then
        rm -f "$tmp"
        echo "backup: the dump does not read back; discarded" >&2
        return 1
    fi
    mv "$tmp" "$final"
    echo "backup: wrote ${final} ($(du -h "$final" | cut -f1))"

    # Retention: keep the newest $keep dumps. The names sort by time.
    local old
    mapfile -t old < <(find "$dir" -maxdepth 1 -name 'stacks-*.dump' -printf '%f\n' | sort -r | tail -n "+$((keep + 1))")
    for name in "${old[@]}"; do
        rm -f -- "${dir:?}/${name}"
        echo "backup: removed ${name} (keeping ${keep})"
    done
}

if [ "${1:-}" = "once" ]; then
    dump
    exit 0
fi

echo "backup: every ${interval}s into ${dir}, keeping ${keep}"
# PID 1 ignores signals it does not trap.
trap 'echo "backup: stopping"; exit 0' TERM INT
while true; do
    # A failed dump is reported and retried at the next interval; the
    # sidecar keeps running so one bad night does not end the schedule.
    dump || true
    sleep "$interval" &
    # `wait` returns on SIGTERM, so `docker compose stop` is immediate.
    wait $!
done
