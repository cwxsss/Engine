#!/usr/bin/env bash
set -euo pipefail

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    echo "neither sha256sum nor shasum is available" >&2
    return 1
  fi
}

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace_root="$(cd "$project_root/../../.." && pwd)"
deveco_contents="${DEVECO_STUDIO_CONTENTS:-/Applications/DevEco-Studio.app/Contents}"
sdk_root="$deveco_contents/sdk"
native_root="$sdk_root/default/openharmony/native"
ohos_arch="${UC_OHOS_ARCH:-arm64-v8a}"
case "$ohos_arch" in
  arm64-v8a)
    target="aarch64-unknown-linux-ohos"
    target_triple="aarch64-linux-ohos"
    ;;
  x86_64)
    target="x86_64-unknown-linux-ohos"
    target_triple="x86_64-linux-ohos"
    ;;
  *)
    echo "unsupported OHOS ABI: $ohos_arch (expected arm64-v8a or x86_64)" >&2
    exit 1
    ;;
esac
target_env="${target//-/_}"
target_env_upper="${target_env^^}"
target_dir="${UC_OHOS_TARGET_DIR:-${TMPDIR:-/tmp}/uniclipboard-ohos-target}"
host_os="$(uname -s)"
if [[ "$host_os" == MINGW* || "$host_os" == MSYS* || "$host_os" == CYGWIN* ]]; then
  # DevEco ships POSIX launcher scripts for target clang on Windows. Cargo and
  # cc-rs need the native Windows binaries plus the same target/sysroot flags.
  rust_linker="$native_root/llvm/bin/clang.exe"
  rust_cxx="$native_root/llvm/bin/clang++.exe"
  rust_ar="$native_root/llvm/bin/llvm-ar.exe"
  cmake="$native_root/build-tools/cmake/bin/cmake.exe"
  ninja="$native_root/build-tools/cmake/bin/ninja.exe"
  ohos_toolchain="$native_root/build/cmake/ohos.toolchain.cmake"
  ohos_sysroot="$native_root/sysroot"
  # RUSTFLAGS is whitespace-delimited. Use an MSYS short path so a default
  # "DevEco Studio" installation cannot split the sysroot linker argument.
  ohos_sysroot_native="$(cygpath -ms "$ohos_sysroot")"
  ohos_cflags="--target=$target_triple --sysroot=$ohos_sysroot_native -D__MUSL__"
  ohos_rustflags="${RUSTFLAGS:-} -C link-arg=--target=$target_triple -C link-arg=--sysroot=$ohos_sysroot_native -C link-arg=-D__MUSL__"
  native_tools_path="$(cygpath -u "$native_root/build-tools/cmake/bin")"
  export PATH="$native_tools_path:$PATH"
  export "CMAKE_TOOLCHAIN_FILE_${target_env}=$ohos_toolchain"
  export "CMAKE_GENERATOR_${target_env}=Ninja"
  export "CMAKE_MAKE_PROGRAM_${target_env}=$ninja"
else
  rust_linker="$native_root/llvm/bin/$target_triple-clang"
  rust_cxx="$native_root/llvm/bin/$target_triple-clang++"
  rust_ar="$native_root/llvm/bin/llvm-ar"
  cmake="$native_root/build-tools/cmake/bin/cmake"
  ohos_cflags=""
  ohos_rustflags="${RUSTFLAGS:-}"
fi
declaration_source="$workspace_root/bindings/uc-ohos-napi/ohos/index.d.ts"
dist_dir="${UC_OHOS_DIST_DIR:-$target_dir/uc-ohos-napi-dist/ohos}"
debug_dir="$(dirname "$dist_dir")/debug-symbols/ohos"
staging_project_root="${UC_OHOS_STAGING_DIR:-${TMPDIR:-/tmp}/uniclipboard-ohos-har-$$}"
har_root="$staging_project_root/engine"
har_library_dir="$har_root/libs/$ohos_arch"
har_declaration_dir="$har_root/src/main/cpp/types/libuc_ohos_napi"
entry_root="$staging_project_root/entry"
entry_library_dir="$entry_root/libs/$ohos_arch"
entry_declaration_dir="$entry_root/src/main/cpp/types/libuc_ohos_napi"

for executable in "$rust_linker" "$rust_cxx" "$rust_ar" "$cmake"; do
  if [[ ! -x "$executable" ]]; then
    echo "required DevEco tool is unavailable: $executable" >&2
    exit 1
  fi
done

export "CARGO_TARGET_${target_env_upper}_LINKER=$rust_linker"
export "CC_${target_env}=$rust_linker"
export "CXX_${target_env}=$rust_cxx"
export "AR_${target_env}=$rust_ar"
export "CFLAGS_${target_env}=$ohos_cflags"
export "CXXFLAGS_${target_env}=$ohos_cflags"
export RUSTFLAGS="$ohos_rustflags"

CARGO_TARGET_DIR="$target_dir" \
CMAKE="$cmake" \
  cargo build \
    --manifest-path "$workspace_root/Cargo.toml" \
    -p uc-ohos-napi \
    --target "$target" \
    --locked

mkdir -p "$har_library_dir" "$har_declaration_dir" "$entry_library_dir"
cp "$project_root/build-profile.json5" "$staging_project_root/build-profile.json5"
cp "$project_root/hvigorfile.ts" "$staging_project_root/hvigorfile.ts"
cp "$project_root/oh-package.json5" "$staging_project_root/oh-package.json5"
cp -R "$project_root/AppScope" "$staging_project_root/AppScope"
cp -R "$project_root/hvigor" "$staging_project_root/hvigor"
cp "$project_root/entry/build-profile.json5" "$entry_root/build-profile.json5"
cp "$project_root/entry/hvigorfile.ts" "$entry_root/hvigorfile.ts"
cp "$project_root/entry/oh-package.json5" "$entry_root/oh-package.json5"
cp -R "$project_root/entry/src/." "$entry_root/src"
cp "$project_root/engine/Index.ets" "$har_root/Index.ets"
cp "$project_root/engine/build-profile.json5" "$har_root/build-profile.json5"
cp "$project_root/engine/hvigorfile.ts" "$har_root/hvigorfile.ts"
cp "$project_root/engine/oh-package.json5" "$har_root/oh-package.json5"
cp "$project_root/engine/src/main/module.json5" "$har_root/src/main/module.json5"
cp "$project_root/engine/src/main/cpp/types/libuc_ohos_napi/oh-package.json5" \
  "$har_declaration_dir/oh-package.json5"
cp "$project_root/engine/src/main/cpp/types/libuc_ohos_napi/package.json" \
  "$har_declaration_dir/package.json"
mkdir -p "$entry_declaration_dir"
cp "$target_dir/$target/debug/libuc_ohos_napi.so" "$har_library_dir/libuc_ohos_napi.so"
cp "$declaration_source" "$har_declaration_dir/index.d.ts"
cp "$target_dir/$target/debug/libuc_ohos_napi.so" "$entry_library_dir/libuc_ohos_napi.so"
cp "$declaration_source" "$entry_declaration_dir/index.d.ts"
mkdir -p "$dist_dir" "$debug_dir"
cp "$har_library_dir/libuc_ohos_napi.so" "$dist_dir/libuc_ohos_napi.so"
cp "$har_library_dir/libuc_ohos_napi.so" "$debug_dir/$ohos_arch.so"
cp "$declaration_source" "$dist_dir/index.d.ts"
sha256_file "$dist_dir/libuc_ohos_napi.so" > "$dist_dir/uc-ohos-napi.checksum.txt"
version="$(cargo pkgid --manifest-path "$workspace_root/Cargo.toml" -p uc-ohos-napi)"
version="${version##*#}"
commit="$(git -C "$workspace_root" rev-parse HEAD)"
printf 'v%s\n' "$version" > "$dist_dir/version.txt"
printf '%s\n' "$commit" > "$dist_dir/source-commit.txt"
printf 'sdk.dir=%s\n' "$sdk_root" > "$staging_project_root/local.properties"

if [[ "${UC_OHOS_SKIP_PACKAGE:-0}" == "1" ]]; then
  echo "OK: $dist_dir/libuc_ohos_napi.so"
  exit 0
fi

cd "$staging_project_root"
"$deveco_contents/tools/ohpm/bin/ohpm" install --all
DEVECO_SDK_HOME="$sdk_root" \
  "$deveco_contents/tools/hvigor/bin/hvigorw" \
  --mode module \
  -p product=default \
  -p module=engine@default \
  -p buildMode=release \
  assembleHar \
  --no-daemon

har_path="$(find "$har_root/build" -type f -name '*.har' -print -quit)"
if [[ -z "$har_path" ]]; then
  echo "HarmonyOS HAR was not produced" >&2
  exit 1
fi
cp "$har_path" "$dist_dir/UniClipboardEngine.har"
har_archive_path="$dist_dir/UniClipboardEngine.har"
if [[ "$host_os" == MINGW* || "$host_os" == MSYS* || "$host_os" == CYGWIN* ]]; then
  har_archive_path="$(cygpath -u "$har_archive_path")"
fi

required_arches=("$ohos_arch")
if [[ "${UC_OHOS_REQUIRE_BOTH:-0}" == "1" ]]; then
  required_arches=(arm64-v8a x86_64)
fi
for architecture in "${required_arches[@]}"; do
  required_path="package/libs/$architecture/libuc_ohos_napi.so"
  if ! tar -tzf "$har_archive_path" | grep -Fx "$required_path" >/dev/null; then
    echo "HarmonyOS HAR is missing $required_path" >&2
    exit 1
  fi
done
for required_path in \
  package/Index.d.ets \
  package/oh-package.json5 \
  package/src/main/cpp/types/libuc_ohos_napi/index.d.ts; do
  if ! tar -tzf "$har_archive_path" | grep -Fx "$required_path" >/dev/null; then
    echo "HarmonyOS HAR is missing $required_path" >&2
    exit 1
  fi
done

har_package_manifest="$(tar -xOzf "$har_archive_path" package/oh-package.json5)"
if ! grep -Fq '"name":"@uniclipboard/engine"' <<< "$har_package_manifest"; then
  echo "HarmonyOS HAR has an unexpected package name" >&2
  exit 1
fi
if ! grep -Fq "\"version\":\"$version\"" <<< "$har_package_manifest"; then
  echo "HarmonyOS HAR version does not match v$version" >&2
  exit 1
fi

sha256_file "$dist_dir/UniClipboardEngine.har" > "$dist_dir/UniClipboardEngine.har.checksum.txt"

echo "OK: $dist_dir/UniClipboardEngine.har"
