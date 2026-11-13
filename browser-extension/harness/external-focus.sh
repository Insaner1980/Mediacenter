#!/usr/bin/env bash
set -eu
source scripts/dev-env.sh
focus_dir=$(mktemp -d /tmp/mediacenter-focus-display.XXXXXX)
for n in {194..210}; do
  if ! test -e "/tmp/.X$n-lock" && ! test -S "/tmp/.X11-unix/X$n"; then break; fi
done
test ! -e "/tmp/.X$n-lock" && test ! -S "/tmp/.X11-unix/X$n"
export DISPLAY=":$n" XAUTHORITY="$focus_dir/xauth" MEDIACENTER_PRIVATE_DISPLAY=":$n"
mkdir -m 700 "$focus_dir/runtime"
export XDG_RUNTIME_DIR="$focus_dir/runtime" XDG_CONFIG_HOME="$focus_dir/config" XDG_DATA_HOME="$focus_dir/data" XDG_CACHE_HOME="$focus_dir/cache" XDG_STATE_HOME="$focus_dir/state" GDK_BACKEND=x11
install -m 600 /dev/null "$XAUTHORITY"
xauth -f "$XAUTHORITY" add "$DISPLAY" . "$(openssl rand -hex 16)"
.tools/xvfb/root/usr/bin/Xvfb "$DISPLAY" -screen 0 1280x900x24 -nolisten tcp -auth "$XAUTHORITY" >"$focus_dir/xvfb.log" 2>&1 &
focus_x=$!
trap 'kill "$focus_x" 2>/dev/null || true; wait "$focus_x" 2>/dev/null || true' EXIT
for i in {1..30}; do if xwininfo -root >/dev/null 2>&1; then break; fi; sleep .1; done
xwininfo -root >/dev/null
dbus-run-session -- bash -c 'metacity --sm-disable --no-composite > "$1/wm.log" 2>&1 & focus_wm=$!; trap "kill $focus_wm 2>/dev/null || true" EXIT; for i in {1..30}; do xprop -root _NET_SUPPORTING_WM_CHECK | rg -q "0x" && break; sleep .1; done; node browser-extension/harness/native-input.mjs --external-focus' _ "$focus_dir" >"$focus_dir/run.log" 2>&1 || status=$?
printf '%s\n' "$focus_dir"
cat .tools/browser-feasibility/native-input-external-focus-result.json
exit "${status:-0}"
