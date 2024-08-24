#!/usr/bin/env bash

source "${BASH_SOURCE%/*}/daemon/common.bash"

echo "PATH: $PATH"

trap 'trap - SIGTERM && kill 0' SIGTERM SIGINT EXIT

rm -f $reload_fifo
mkfifo $reload_fifo

reload() {
    # This just kind of assumes recompilation doesn't take too long.
    kill "$rkgk_pid"
    just port=8083 profile=release serve > "$build_log" 2>&1 &
    rkgk_pid="$!"
}

reload

while true; do
    read command < "$reload_fifo"
    case "$command" in
        reload)
            echo "Reloading"
            reload;;
    esac
done
