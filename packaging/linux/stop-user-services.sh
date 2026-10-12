#!/bin/sh
# Run as root when OpenCreative is removed (not upgraded): stop every
# logged-in user's background service, which shows the top-bar icon, and
# close open app windows, so nothing keeps running from deleted files.
if command -v loginctl >/dev/null 2>&1 && command -v systemctl >/dev/null 2>&1; then
    loginctl list-users --no-legend 2>/dev/null | while read -r _uid user _rest; do
        [ -n "$user" ] || continue
        systemctl --user -M "$user@" stop opencreative.service >/dev/null 2>&1 || true
    done
fi
# Copies started outside systemd: only the packaged programs, by full path.
if command -v pkill >/dev/null 2>&1; then
    pkill -f '^/usr/bin/opencreative(-gui)?( |$)' >/dev/null 2>&1 || true
fi
exit 0
