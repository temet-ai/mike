#!/usr/bin/env bash
# The compile step of `scripts/release.sh --deploy-build` (scripts/build-local-release
# --deploy): mike and mike-mcp for a deploy to our own machines, never a release.
#
#   deploy-archive-build.sh <target> <cargo target> <path-remap config> <build dir>
#
# <cargo target> is <target>.<glibc> for a cargo-zigbuild cross build. It builds the infra
# profile through the mbx cache into target/mbx-deploy-<target>, on half the processors at
# the lowest priority, so a commit restores every unit it
# did not change. mike compiles with the fixed mbx identity and is then stamped with HEAD
# (scripts/lib/stamp_build_identity.py, built_at from SOURCE_DATE_EPOCH). mike-mcp has no
# stamp slot and depends on no workspace crate, so it compiles with the commit's identity:
# only that one small crate recompiles per commit. Writes <build dir>/<target>/dist/mike and
# mike-mcp as new files, never hard links into the store.
set -euo pipefail

target="${1:?target}" cargo_target="${2:?cargo target}" path_config="${3:?path config}" out="${4:?build dir}"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=preflight-toolchain.sh
source "$root/scripts/lib/preflight-toolchain.sh"
# shellcheck source=heavy-build.sh
source "$root/scripts/lib/heavy-build.sh"

epoch="${SOURCE_DATE_EPOCH:?the release preparer sets SOURCE_DATE_EPOCH}"
# build_identity.rs reruns on SOURCE_DATE_EPOCH, so leaving it set would change the key of
# every workspace crate on every commit. The release environment's single job is for a
# reproducible release; a deploy uses half the processors.
unset SOURCE_DATE_EPOCH CARGO_BUILD_JOBS
jobs="$(heavy_build_jobs)"
priority=()
while IFS= read -r word; do priority+=("$word"); done < <(heavy_build_priority)

prepare_mbx_build_cache "$root" "$root/target/mbx-deploy-$target"
build_dir="$CARGO_TARGET_DIR"
args=(--offline --locked --profile infra --target "$cargo_target" --config "$path_config")
if [[ "$cargo_target" == "$target" ]]; then
  cargo_cmd=("$MBX_BIN" build)
else
  shim="$(mbx_cargo_shim)"
  cargo_cmd=(env "CARGO=$shim" cargo-zigbuild zigbuild)
fi

started=$SECONDS
CARGO_BUILD_JOBS="$jobs" "${priority[@]}" "${cargo_cmd[@]}" "${args[@]}" -p mike
(
  unset MIKE_BUILD_CACHE
  CARGO_BUILD_JOBS="$jobs" MIKE_BUILD_IDENTITY=git SOURCE_DATE_EPOCH="$epoch" \
    "${priority[@]}" "${cargo_cmd[@]}" "${args[@]}" -p mike-mcp
)
echo "==> Compiled mike and mike-mcp ($jobs jobs) in $((SECONDS - started))s"

dest="$out/$target/dist"
mkdir -p "$dest"
for name in mike mike-mcp; do
  rm -f "$dest/$name"
  cp "$build_dir/$target/infra/$name" "$dest/$name"
  chmod 755 "$dest/$name"
done
echo "==> Stamped mike: $(SOURCE_DATE_EPOCH="$epoch" python3 "$root/scripts/lib/stamp_build_identity.py" "$dest/mike" --repo "$root")"
