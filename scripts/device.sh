#!/usr/bin/env bash

DEVICE_HOST="${SUPERBIRD_HOST:-bridgething.local}"

DEVICE_SSH_OPTS=(
    -o AddressFamily=inet
    -o UserKnownHostsFile=/dev/null
    -o GlobalKnownHostsFile=/dev/null
    -o StrictHostKeyChecking=no
    -o ConnectTimeout=5
    -o LogLevel=ERROR
)

device_ssh() { ssh "${DEVICE_SSH_OPTS[@]}" "root@${DEVICE_HOST}" "$@"; }

device_scp() { scp "${DEVICE_SSH_OPTS[@]}" "$@"; }

device_rsh() { echo "ssh ${DEVICE_SSH_OPTS[*]}"; }
