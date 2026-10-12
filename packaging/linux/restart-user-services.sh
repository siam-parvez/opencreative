#!/bin/sh
# Run as root after an install or upgrade: every logged-in user whose
# OpenCreative service is running gets it restarted on the new version, so
# nobody has to do it by hand. Users without it running are left alone.
command -v loginctl >/dev/null 2>&1 || exit 0
command -v systemctl >/dev/null 2>&1 || exit 0
loginctl list-users --no-legend 2>/dev/null | while read -r _uid user _rest; do
    [ -n "$user" ] || continue
    systemctl --user -M "$user@" daemon-reload >/dev/null 2>&1 || true
    systemctl --user -M "$user@" try-restart opencreative.service >/dev/null 2>&1 || true
done
exit 0
