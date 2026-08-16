#!/usr/bin/env sh
# Create an external SolKit project from the maintained starter template.
set -eu
LC_ALL=C
export LC_ALL

usage() {
    cat <<'EOF'
Usage: scripts/new-solkit-project.sh <destination> <package-name> <app-id>

Creates an external project from templates/solkit-starter.

  destination   New directory outside this SOL checkout.
  package-name  Lowercase Cargo package name using letters, digits, and hyphens.
  app-id        Lowercase reverse-DNS application ID, for example com.example.notes.
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

validate_app_id() {
    app_id=$1
    [ "${#app_id}" -le 255 ] || fail "application ID must not exceed 255 bytes"
    case "$app_id" in
        '' | *[!a-z0-9.-]* | .* | *. | *..*)
            fail "application ID must contain at least two non-empty reverse-DNS components"
            ;;
    esac

    component_count=0
    old_ifs=$IFS
    IFS=.
    set -- $app_id
    IFS=$old_ifs
    for component in "$@"; do
        component_count=$((component_count + 1))
        case "$component" in
            *[!a-z0-9-]* | *- | [!a-z]*)
                fail "application ID components must start with a lowercase letter, end with a letter or digit, and use only lowercase letters, digits, and hyphens"
                ;;
        esac
    done
    [ "$component_count" -ge 2 ] || fail "application ID must contain at least two reverse-DNS components"
}

if [ "${1:-}" = "--help" ] && [ "$#" -eq 1 ]; then
    usage
    exit 0
fi

[ "$#" -eq 3 ] || {
    usage >&2
    exit 1
}

destination_input=$1
package_name=$2
app_id=$3
crate_name=$(printf '%s' "$package_name" | tr '-' '_')

validate_package_name "$package_name"
validate_app_id "$app_id"

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
repo_root=$(CDPATH= cd -- "$script_dir/.." && pwd)
template_dir="$repo_root/templates/solkit-starter"
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

staging_dir=$(mktemp -d "$parent_dir/.solkit-project.XXXXXX")
cleanup() {
    [ -z "${staging_dir:-}" ] || rm -rf "$staging_dir"
}
trap cleanup EXIT HUP INT TERM

project_dir="$staging_dir/project"
mkdir "$project_dir"
cp -R "$template_dir/." "$project_dir"
rm -rf "$project_dir/target"
sed -i \
    -e "s|name = \"solkit-starter\"|name = \"$package_name\"|" \
    -e "s|../../sdk|$repo_root/sdk|g" \
    "$project_dir/Cargo.toml"
sed -i "s|name = \"solkit-starter\"|name = \"$package_name\"|" "$project_dir/Cargo.lock"
sed -i "s|org.example.starter|$app_id|" "$project_dir/src/lib.rs"
sed -i "s|solkit_starter|$crate_name|g" "$project_dir/src/main.rs"

mv "$project_dir" "$destination"
rmdir "$staging_dir"
staging_dir=

printf 'Created SolKit project: %s\n' "$destination"
printf 'Package: %s\nApplication ID: %s\n' "$package_name" "$app_id"
printf 'Next: cargo test --manifest-path %s/Cargo.toml\n' "$destination"
