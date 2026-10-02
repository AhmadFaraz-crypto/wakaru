#!/usr/bin/env bash
set -u

output_path=""
mode=""
while (($# > 0)); do
  case "$1" in
    --output_path) output_path="$2"; shift 2 ;;
    base|new) mode="$1"; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
if [[ -z "$output_path" || -z "$mode" ]]; then
  echo "usage: $0 --output_path <path> base|new" >&2
  exit 2
fi
[[ "$output_path" = /* ]] || output_path="$PWD/$output_path"
mkdir -p "$(dirname "$output_path")"
config_path="$(mktemp)"
trap 'rm -f "$config_path"' EXIT
printf '[profile.default.junit]\npath = "%s"\n' "$output_path" >"$config_path"
status=0
if [[ "$mode" == base ]]; then
  cargo nextest run -p wakaru-core --test un_type_constructor_rule --config-file "$config_path" || status=$?
else
  cargo nextest run -p wakaru-core \
    --test un_type_constructor_coercions_8b3f1d \
    --config-file "$config_path" || status=$?
fi
if [[ ! -s "$output_path" ]]; then
  printf '<testsuites tests="0" failures="1"><testsuite name="runner"><testcase name="runner"><failure message="missing JUnit output"/></testcase></testsuite></testsuites>\n' >"$output_path"
fi
exit "$status"
