#!/usr/bin/env bash
# Veröffentlicht nrzVPS als Multi-Arch-Image in der Registry von nrzGit:
#   100.78.107.162:8390/nicorenezweers/nrz-vps:<version>
# Ein Push durch eine Person lässt nrzGit (nrzStore.json) Image, Digest und Version im nrzStore
# selbst nachtragen.
#
#   scripts/release.sh 1.0.1
#
# Braucht: cargo-zigbuild + zig, dioxus-cli unter ~/.cargo/bin/dx, docker mit buildx.
# Registry-Anmeldung: $NRZGIT_REGISTRY_AUTH (base64 von user:token), $NRZGIT_TOKEN oder der
# Git-extraHeader von nrzCode.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${1:?Version angeben, z. B. 1.0.1}"
REGISTRY="100.78.107.162:8390"
IMAGE="${NRZVPS_IMAGE:-${REGISTRY}/nicorenezweers/nrz-vps}"
DX="${DX:-$HOME/.cargo/bin/dx}"

grep -q "^version = \"${VERSION}\"" Cargo.toml || { echo "Cargo.toml hat nicht Version ${VERSION}" >&2; exit 1; }

# Pfade ohne Leerzeichen (zig-Wrapper und C-Builds vertragen keine)
BUILD="${NRZVPS_BUILD_DIR:-/tmp/nrzvps-build}"
mkdir -p "${BUILD}/bin"
printf '#!/bin/sh\nexec zig ar "$@"\n' > "${BUILD}/bin/zig-ar"
printf '#!/bin/sh\nexec zig ranlib "$@"\n' > "${BUILD}/bin/zig-ranlib"
chmod +x "${BUILD}/bin/zig-ar" "${BUILD}/bin/zig-ranlib"
export AR="${BUILD}/bin/zig-ar" RANLIB="${BUILD}/bin/zig-ranlib" CARGO_ZIGBUILD_CACHE_DIR="${BUILD}/zigcache"
# rust-objcopy von dx findet libLLVM sonst nicht
export DYLD_FALLBACK_LIBRARY_PATH="$(rustc --print sysroot)/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"

# ── Bauen ──
rm -rf dist && mkdir -p dist
if [ "${SKIP_WEB:-}" != 1 ] || [ ! -f "${BUILD}/webdist/public/index.html" ]; then
  rm -rf "${BUILD}/webdist"
  (cd crates/web && CARGO_TARGET_DIR="${BUILD}/wasm" "${DX}" bundle --platform web --release --out-dir "${BUILD}/webdist")
  # Speicher sparen: WASM-Zwischenstände wegwerfen
  [ "${KEEP_BUILD:-}" = 1 ] || rm -rf "${BUILD}/wasm"
fi
test -f "${BUILD}/webdist/public/index.html"
cp -R "${BUILD}/webdist/public" dist/public
for pair in amd64:x86_64-unknown-linux-gnu arm64:aarch64-unknown-linux-gnu; do
  arch="${pair%%:*}"; target="${pair#*:}"
  CARGO_TARGET_DIR="${BUILD}/linux" cargo zigbuild --release -p nrz-vps --target "${target}.2.36"
  cp "${BUILD}/linux/${target}/release/nrz-vps" "dist/nrz-vps-${arch}"
  # Speicher sparen: Zwischenstände dieses Targets wegwerfen
  [ "${KEEP_BUILD:-}" = 1 ] || rm -rf "${BUILD}/linux/${target}"
done

# ── Registry-Anmeldung (temporäre Docker-Konfiguration, ~/.docker bleibt unberührt) ──
AUTH="${NRZGIT_REGISTRY_AUTH:-}"
if [ -z "${AUTH}" ] && [ -n "${NRZGIT_TOKEN:-}" ]; then
  AUTH="$(printf '%s:%s' "${NRZGIT_USERNAME:-nicorenezweers}" "${NRZGIT_TOKEN}" | base64)"
fi
if [ -z "${AUTH}" ]; then
  for i in $(seq 0 $(( ${GIT_CONFIG_COUNT:-0} - 1 ))); do
    k="GIT_CONFIG_KEY_${i}"; v="GIT_CONFIG_VALUE_${i}"
    case "${!k}" in *"${REGISTRY}"*[eE]xtra[hH]eader*) AUTH="${!v##*Basic }";; esac
  done
fi
[ -n "${AUTH}" ] || { echo "Keine Registry-Anmeldung (NRZGIT_REGISTRY_AUTH oder NRZGIT_TOKEN setzen)" >&2; exit 1; }
ORIG_CONFIG="${DOCKER_CONFIG:-$HOME/.docker}"
export DOCKER_CONFIG="$(mktemp -d)"
trap 'rm -rf "${DOCKER_CONFIG}"' EXIT
for d in contexts cli-plugins buildx; do
  [ -e "${ORIG_CONFIG}/${d}" ] && ln -s "${ORIG_CONFIG}/${d}" "${DOCKER_CONFIG}/${d}"
done
python3 - "${ORIG_CONFIG}/config.json" "${DOCKER_CONFIG}/config.json" "${REGISTRY}" "${AUTH}" <<'PY'
import json, sys
src, dst, reg, auth = sys.argv[1:]
try:
    cfg = json.load(open(src))
except Exception:
    cfg = {}
cfg.pop("credsStore", None)
cfg.setdefault("auths", {})[reg] = {"auth": auth}
json.dump(cfg, open(dst, "w"))
PY

# ── Image ──
docker buildx inspect nrzvps-builder >/dev/null 2>&1 \
  || docker buildx create --name nrzvps-builder --driver docker-container --config docker/buildkitd.toml >/dev/null
docker buildx build --builder nrzvps-builder --platform linux/amd64,linux/arm64 \
  -t "${IMAGE}:${VERSION}" --push .
docker buildx imagetools inspect --builder nrzvps-builder "${IMAGE}:${VERSION}" | head -5
echo "Fertig: ${IMAGE}:${VERSION}"
