#!/usr/bin/env bash
# Build, install and launch the native Rust HarmonyOS host.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -f "$ROOT/scripts/dev.local.env" ]]; then source "$ROOT/scripts/dev.local.env"; fi
HDC="${PHOTOCRAFT_HDC:-/Applications/DevEco-Studio.app/Contents/sdk/default/openharmony/toolchains/hdc}"
HVIGOR="${PHOTOCRAFT_HVIGOR:-/Applications/DevEco-Studio.app/Contents/tools/hvigor/bin/hvigorw}"
OHPM="${PHOTOCRAFT_OHPM:-/Applications/DevEco-Studio.app/Contents/tools/ohpm/bin/ohpm}"
export DEVECO_SDK_HOME="${DEVECO_SDK_HOME:-/Applications/DevEco-Studio.app/Contents/sdk}"
export JAVA_HOME="${JAVA_HOME:-/Applications/DevEco-Studio.app/Contents/jbr/Contents/Home}"
# PackageHap shells out to `java` by name (JAVA_HOME alone is not enough on Windows), and
# hvigor itself is a node script, so both have to be on PATH before it runs.
export PATH="$JAVA_HOME/bin:/Applications/DevEco-Studio.app/Contents/tools/node/bin:$PATH"
BUNDLE="${PHOTOCRAFT_BUNDLE:-io.github.shaliuxin.photocraft.hmos}"
# ABIs to build and package. Both by default, so one HAP covers ARM devices and the x86_64
# phone/PC emulator images; set PHOTOCRAFT_ABIS to a single ABI for a faster loop.
ABIS="${PHOTOCRAFT_ABIS:-arm64-v8a x86_64}"
triple_for() {
  case "$1" in
    arm64-v8a) echo aarch64-unknown-linux-ohos ;;
    x86_64) echo x86_64-unknown-linux-ohos ;;
    armeabi-v7a) echo armv7-unknown-linux-ohos ;;
    *) echo "Unknown ABI: $1" >&2; return 1 ;;
  esac
}
TARGETS=""
for abi in $ABIS; do TARGETS="$TARGETS $(triple_for "$abi")"; done
one_device() {
  local count
  count="$("$HDC" list targets | tr -d '\r' | awk 'NF && $0 != "[Empty]" {n++} END {print n+0}')"
  [[ "$count" == 1 ]] || { echo "Connect exactly one simulator/device ($count found)" >&2; exit 1; }
}
build() {
  # shellcheck disable=SC2086
  PHOTOCRAFT_NATIVE_TARGETS="$TARGETS" "$ROOT/scripts/build_native.sh"
  cd "$ROOT"
  "$OHPM" install
  "$HVIGOR" assembleHap --mode module -p product=default -p buildMode=debug --no-daemon
  # shellcheck disable=SC2086
  python3 "$ROOT/scripts/verify_native.py" "$ROOT/entry/build/default/outputs/default/entry-default-unsigned.hap" \
    --expected-bundle "$BUNDLE" --abis $ABIS
}
install() {
  one_device
  local hap="$ROOT/entry/build/default/outputs/default/entry-default-signed.hap"
  [[ -f "$hap" ]] || { echo 'Configure local debug signing in DevEco Studio' >&2; exit 1; }
  local result
  result="$("$HDC" install -r "$hap")"
  echo "$result"
  [[ "$result" == *'install bundle successfully'* ]] || exit 1
}
launch() {
  one_device
  # Relaunch only after saving the current documents.
  "$HDC" shell aa force-stop "$BUNDLE" >/dev/null 2>&1 || true
  "$HDC" shell aa start -a EntryAbility -b "$BUNDLE"
}
case "${1:-run}" in
  build) build ;;
  run) build; install; launch ;;
  launch) launch ;;
  *) echo 'Usage: scripts/dev.sh [build|run|launch]' >&2; exit 2 ;;
esac
