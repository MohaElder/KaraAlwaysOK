#!/bin/zsh
# Starts the already-built app with tauri:dev, waits for its page, saves a picture of the window
# to $1, runs $2 (if given) while the app is open, quits it, and fails when the webview or Rust
# reported an error. Finishes within about 90 s; every process it starts is gone when it exits.
set -u
shot=${1:A}
log=${shot:r}.log
cd "${0:A:h}/.."
mkdir -p "${shot:h}"
rm -f "$shot" "${shot:r}.while-open.txt"
fail() { echo "FAIL: $1 Output: $log"; exit 1; }

pkill -x kara-app && sleep 1
[[ -n $(swift scripts/window-id.swift kara-app KaraAlwaysOK) ]] && fail "An old KaraAlwaysOK window is still open."
lsof -ti tcp:1420 >/dev/null && fail "Port 1420 is in use; stop the old dev server (lsof -i tcp:1420)."

perl -MPOSIX -e 'setsid(); exec @ARGV' npm run tauri:dev >"$log" 2>&1 &
dev=$!
stop() {
  kill -- -$dev 2>/dev/null
  pkill -x kara-app
  sleep 2
  kill -9 -- -$dev 2>/dev/null
  pkill -9 -x kara-app
}
trap stop EXIT
trap "exit 130" INT TERM

ready=""
for _ in {1..60}; do
  grep -q '^\[webview-ready\]' "$log" && ready=1 && break
  kill -0 $dev 2>/dev/null || break
  sleep 1
done
[[ -n $ready ]] || fail "The page never loaded within 60 s (build first: cargo build -p kara-app)."
id=$(swift scripts/window-id.swift kara-app KaraAlwaysOK)
[[ -n $id ]] || fail "The page loaded but no app window was found."
sleep 1
caffeinate -u -t 2
screencapture -x -o -l "$id" "$shot"
[[ -n ${2:-} ]] && eval "$2" >"${shot:r}.while-open.txt" 2>&1
stop
trap - EXIT INT TERM

grep -nE '^\[webview\]|panicked|^error' "$log" && fail "Errors above."
[[ -s $shot ]] || fail "No picture was saved (Screen Recording permission, or the display is asleep). No errors were logged."
echo "OK. Picture: $shot"
