#!/usr/bin/env sh
# Create an external SolKit component library from the maintained template.
set -eu
LC_ALL=C
export LC_ALL

usage() {
    cat <<'EOF'
Usage: scripts/new-solkit-component.sh <destination> <package-name>

Creates an external library from templates/solkit-component.

  destination   New directory outside this SOL checkout.
  package-name  Lowercase Cargo package name using letters, digits, and hyphens.
EOF
}

fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

validate_package_name() {
    package_name=$1
    case "$package_name" in
        '' | *[!a-z0-9-]* | *- | [!a-z]*)
            fail "package name must start with a lowercase letter, end with a letter or digit, and use only lowercase letters, digits, and hyphens"
            ;;
    esac
}

if [ "${1:-}" = "--help" ] && [ "$#" -eq 1 ]; then
    usage
    exit 0
fi

[ "$#" -eq 2 ] || {
    usage >&2
    exit 1
}

destination_input=$1
package_name=$2
validate_package_name "$package_name"

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
template_dir="$repo_root/templates/solkit-component"
parent_dir=$(dirname -- "$destination_input")
project_name=$(basename -- "$destination_input")

[ -d "$parent_dir" ] || fail "destination parent directory does not exist: $parent_dir"
[ "$project_name" != "." ] && [ "$project_name" != ".." ] || fail "destination must name a new project directory"
parent_dir=$(CDPATH= cd -- "$parent_dir" && pwd -P)
destination="$parent_dir/$project_name"

case "$destination" in
    "$repo_root" | "$repo_root"/*)
        fail "destination must be outside the SOL checkout"
        ;;
esac

[ ! -e "$destination" ] || fail "destination already exists: $destination"

staging_dir=$(mktemp -d "$parent_dir/.solkit-component.XXXXXX")
cleanup() {
    [ -z "${staging_dir:-}" ] || rm -rf "$staging_dir"
}
trap cleanup EXIT HUP INT TERM

project_dir="$staging_dir/project"
mkdir "$project_dir"
cp -R "$template_dir/." "$project_dir"
rm -rf "$project_dir/target"
sed -i \
    -e "s|name = \"solkit-component\"|name = \"$package_name\"|" \
    -e "s|../../sdk|$repo_root/sdk|g" \
    "$project_dir/Cargo.toml"
sed -i "s|name = \"solkit-component\"|name = \"$package_name\"|" "$project_dir/Cargo.lock"

mv "$project_dir" "$destination"
rmdir "$staging_dir"
staging_dir=

printf 'Created SolKit component library: %s\n' "$destination"
printf 'Package: %s\n' "$package_name"
printf 'Next: cargo test --manifest-path %s/Cargo.toml\n' "$destination"
