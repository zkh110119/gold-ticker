#!/usr/bin/env bash
set -euo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root_dir"

app_name="GoldTicker"
executable_name="gold-ticker"
bundle_id="com.goldticker.app"
dist_dir="$root_dir/dist"
app_path="$dist_dir/$app_name.app"
info_template="$root_dir/packaging/Info.plist"
icon="$root_dir/packaging/GoldTicker.icns"
staging_dir=""
dmgbuild_venv=""

cleanup() {
    [[ -z "$staging_dir" ]] || rm -rf "$staging_dir"
    [[ -z "$dmgbuild_venv" ]] || rm -rf "$dmgbuild_venv"
}
trap cleanup EXIT

command -v cargo >/dev/null
command -v plutil >/dev/null
command -v hdiutil >/dev/null
command -v shasum >/dev/null
command -v xattr >/dev/null
command -v codesign >/dev/null
command -v python3 >/dev/null

mkdir -p "$dist_dir"
dmgbuild_venv="$dist_dir/.dmgbuild-venv-$RANDOM"
python3 -m venv "$dmgbuild_venv"
"$dmgbuild_venv/bin/python" -m pip install --disable-pip-version-check --quiet 'dmgbuild==1.6.5'

if [[ ! -f "$icon" ]]; then
    printf 'Missing %s. Run packaging/generate-icon.sh first.\n' "$icon" >&2
    exit 1
fi

version=$(cargo pkgid | sed 's/.*#//')
build_number=${BUILD_NUMBER:-$version}
architecture=$(uname -m)
dmg_filename="$app_name-$version-$architecture.dmg"
dmg_path="$dist_dir/$dmg_filename"
checksum_path="$dmg_path.sha256"

mkdir -p "$dist_dir"
cargo build --release

rm -rf "$app_path"
mkdir -p "$app_path/Contents/MacOS" "$app_path/Contents/Resources"
cp "$info_template" "$app_path/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "$bundle_id" "$app_path/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$app_path/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$build_number" "$app_path/Contents/Info.plist"
cp "$root_dir/target/release/$executable_name" "$app_path/Contents/MacOS/$executable_name"
cp "$icon" "$app_path/Contents/Resources/GoldTicker.icns"
chmod 755 "$app_path/Contents/MacOS/$executable_name"
xattr -cr "$app_path"
codesign --force --deep --sign - "$app_path"
plutil -lint "$app_path/Contents/Info.plist"

staging_dir=$(mktemp -d "$dist_dir/$app_name-dmg-XXXXXXXX")
cp -R "$app_path" "$staging_dir/$app_name.app"
ln -s /Applications "$staging_dir/Applications"
[[ -d "$staging_dir/$app_name.app" && -L "$staging_dir/Applications" ]]

rm -f "$dmg_path" "$checksum_path"
"$dmgbuild_venv/bin/python" -m dmgbuild \
    -s "$root_dir/packaging/dmg-settings.py" \
    -D source_folder="$staging_dir" \
    -D root_dir="$root_dir" \
    "GoldTicker" \
    "$dmg_path"
hdiutil verify "$dmg_path"

final_attach_output=$(hdiutil attach -plist -nobrowse -readonly "$dmg_path")
final_device=$(printf '%s' "$final_attach_output" | plutil -convert json -o - -- - | python3 -c 'import json, sys; data=json.load(sys.stdin); print(next((e.get("dev-entry", "") for e in data.get("system-entities", []) if e.get("mount-point")), ""))')
final_path=$(printf '%s' "$final_attach_output" | plutil -convert json -o - -- - | python3 -c 'import json, sys; data=json.load(sys.stdin); print(next((e.get("mount-point", "") for e in data.get("system-entities", []) if e.get("mount-point")), ""))')
[[ -n "$final_device" && -n "$final_path" ]] || {
    printf 'Unable to determine the final DMG mount from hdiutil attach output.\n' >&2
    exit 1
}
[[ -d "$final_path/$app_name.app" && -L "$final_path/Applications" ]] || {
    printf 'Final DMG content validation failed.\n' >&2
    hdiutil detach "$final_device" -quiet || true
    exit 1
}
[[ "$(readlink "$final_path/Applications")" == "/Applications" ]] || {
    printf 'Final DMG Applications link validation failed.\n' >&2
    hdiutil detach "$final_device" -quiet || true
    exit 1
}
[[ -f "$final_path/.DS_Store" ]] || {
    printf 'Final DMG Finder metadata validation failed.\n' >&2
    hdiutil detach "$final_device" -quiet || true
    exit 1
}
hdiutil detach "$final_device" -quiet

(
    cd "$dist_dir"
    shasum -a 256 "$dmg_filename"
) > "$checksum_path"

printf 'Ad-hoc signed, unnotarized DMG artifact: %s\n' "$dmg_path"
printf 'SHA-256 checksum: %s\n' "$checksum_path"
printf 'Upload both files to the matching GitHub Release after completing the release checklist.\n'
