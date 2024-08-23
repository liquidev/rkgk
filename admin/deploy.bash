#!/usr/bin/env bash

ssh "$RKGK_SERVER" -p "$RKGK_SERVER_PORT" "bash" "~/repo/admin/daemon/deploy.bash"
