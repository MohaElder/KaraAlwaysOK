#!/bin/zsh
# Builds the app with its page inside, runs that build on its own (scratch data, its own target folder and process), saves a
# picture of its window to $1, runs $2 (if given) while it is open, then stops only that process. Fails when it didn't build,
# start or show a window, or logged a panic or an error. Never touches port 1420 or another running KaraAlwaysOK.
set -u
shot=${1:A}
log=${shot:r}.log
cd "${0:A:h}/.."
work=${PWD:h}/.superpowers/sdd/2026-09-27-phase2-phone-mics
mkdir -p "${shot:h}" "$work/data"
rm -f "$shot" "${shot:r}.while-open.txt"
export KARA_PHONE_PORT=${KARA_PHONE_PORT:-8543}
fail() { echo "FAIL: $1 Output: $log"; exit 1; }

npm run build >"$log" 2>&1 || fail "The page didn't build."
source "$HOME/.cargo/env"
CARGO_TARGET_DIR="$work/target" cargo build -p kara-app --features tauri/custom-protocol >>"$log" 2>&1 || fail "The app didn't build."
KARA_DATA="$work/data" "$work/target/debug/kara-app" >>"$log" 2>&1 &
app=$!
trap 'kill $app 2>/dev/null; sleep 2; kill -9 $app 2>/dev/null' EXIT
trap "exit 130" INT TERM

id=""
for _ in {1..60}; do
  id=$(swift scripts/window-id.swift $app)
  [[ -n $id ]] && break
  kill -0 $app 2>/dev/null || fail "The app quit on its own."
  sleep 1
done
[[ -n $id ]] || fail "No window within 60 s."
sleep 2
caffeinate -u -t 2
screencapture -x -o -l "$id" "$shot"
[[ -n ${2:-} ]] && eval "$2" >"${shot:r}.while-open.txt" 2>&1
kill $app 2>/dev/null
sleep 2
kill -9 $app 2>/dev/null
trap - EXIT INT TERM

sed -n '/Finished/,$p' "$log" | grep -nE 'panicked|^error' && fail "Errors above."
[[ -s $shot ]] || fail "No picture was saved (Screen Recording permission, or the display is asleep)."
echo "OK. Picture: $shot"
