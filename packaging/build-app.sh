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

cleanup() {
    [[ -z "$staging_dir" ]] || rm -rf "$staging_dir"
}
trap cleanup EXIT

command -v cargo >/dev/null
command -v plutil >/dev/null
command -v hdiutil >/dev/null
command -v shasum >/dev/null
command -v xattr >/dev/null
command -v codesign >/dev/null

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
hdiutil create -quiet -volname "$app_name" -srcfolder "$staging_dir" -format UDZO -ov "$dmg_path"
hdiutil verify "$dmg_path"
(
    cd "$dist_dir"
    shasum -a 256 "$dmg_filename"
) > "$checksum_path"

printf 'Ad-hoc signed, unnotarized DMG artifact: %s\n' "$dmg_path"
printf 'SHA-256 checksum: %s\n' "$checksum_path"
printf 'Upload both files to the matching GitHub Release after completing the release checklist.\n'
