#!/usr/bin/env bash
# Download only published Linux x86-64 release artifacts. No sudo, profile edits,
# cloning, project initialization or indexing. Prompts always use /dev/tty.
set -euo pipefail

die() { printf 'n4h: %s\n' "$*" >&2; exit 1; }
usage() {
  cat <<'HELP'
Usage: bash install.sh [options]
  --version vX.Y.Z       Pin a release (default: latest)
  --bin-dir PATH        Binary directory (default: $HOME/.local/bin)
  --harness LIST        claude-code,codex,opencode (comma-separated)
  --scope user|project  Skill scope; project means the current directory
  --cli-only            Skip skills entirely
  --yes                 Noninteractive confirmation; requires explicit integration choices
  --replace-binary      Explicitly permit replacing a different regular n4h binary
  --help                Show this help
Without --yes, confirm the binary destination, then select skills in n4h setup.
Requirements: Bash, curl, tar, sha256sum, GNU libc >= 2.35; Linux x86-64.
HELP
}

# Parse the full installation body before running it, so an interrupted bootstrap
# download cannot execute a half-received sequence of installation commands.
main() {
version=latest
bin_dir=${HOME:?HOME must be set}/.local/bin
harness=
scope=
cli_only=0
yes=0
replace=0
while (($#)); do
  case "$1" in
    --version|--bin-dir|--harness|--scope)
      (($# >= 2)) && [[ -n "$2" ]] || die "Missing value for $1"
      case "$1" in
        --version) version=$2 ;;
        --bin-dir) bin_dir=$2 ;;
        --harness) harness=$2 ;;
        --scope) scope=$2 ;;
      esac
      shift 2 ;;
    --cli-only) cli_only=1; shift ;;
    --yes) yes=1; shift ;;
    --replace-binary) replace=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) die "Unknown option: $1 (see --help)" ;;
  esac
done
[[ "$version" == latest || "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]] || die 'Version must be latest or a vX.Y.Z release tag.'
[[ "$bin_dir" == /* ]] || die '--bin-dir must be absolute.'
[[ -z "$scope" || "$scope" == user || "$scope" == project ]] || die 'Scope must be user or project.'
if [[ -n "$harness" ]]; then
  [[ "$harness" =~ ^(claude-code|codex|opencode)(,(claude-code|codex|opencode))*$ ]] || die 'Invalid harness list.'
fi
if ((cli_only)); then
  [[ -z "$harness$scope" ]] || die '--cli-only conflicts with --harness/--scope.'
elif ((yes)); then
  [[ -n "$harness" && -n "$scope" ]] || die '--yes requires --harness and --scope, or --cli-only.'
fi
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || die 'Only Linux x86-64 releases are available. See README for source builds.'
for dependency in curl tar sha256sum awk mktemp getconf; do
  command -v "$dependency" >/dev/null || die "Required command not found: $dependency"
done
libc_version=$(getconf GNU_LIBC_VERSION 2>/dev/null) || die 'A GNU libc release is required (musl is not supported).'
libc_version=${libc_version#glibc }
libc_major=${libc_version%%.*}
libc_minor=${libc_version#*.}; libc_minor=${libc_minor%%.*}
[[ "$libc_major" =~ ^[0-9]+$ && "$libc_minor" =~ ^[0-9]+$ ]] || die 'Could not determine GNU libc version.'
((libc_major > 2 || (libc_major == 2 && libc_minor >= 35))) || die 'Release requires GNU libc >= 2.35. Use a source build on older systems.'

# Refuse existing symlinks, including dangling ones. Like setup, this assumes
# directories are controlled by the installing user, not a concurrent attacker.
safe_path() {
  local path=$1
  while [[ "$path" != / ]]; do
    [[ ! -L "$path" ]] || die "Refusing symlink destination: $path"
    path=$(dirname -- "$path")
  done
}
safe_path "$bin_dir/n4h"
if [[ -e "$bin_dir/n4h" ]]; then
  [[ -f "$bin_dir/n4h" ]] || die 'Existing n4h destination is not a regular file.'
  ((replace)) || die 'n4h already exists. Use --replace-binary only if you intend to replace it.'
fi
printf 'n4h: binary destination: %s/n4h\n' "$bin_dir" >&2
if ((cli_only)); then
  printf 'n4h: CLI only; no harness files will be changed.\n' >&2
else
  printf 'n4h: skills: %s; scope: %s (selected/confirmed separately by setup).\n' "${harness:-choose interactively}" "${scope:-choose interactively}" >&2
fi
if ((!yes)); then
  answer=
  read -r -p 'Download and install the binary here? [y/N] ' answer </dev/tty || die 'No terminal. Pass explicit choices with --yes for unattended installation.'
  [[ "$answer" == y || "$answer" == Y || "$answer" == yes ]] || die 'Installation cancelled.'
fi

work=$(mktemp -d)
staged=
cleanup() {
  [[ -z "$staged" ]] || rm -f -- "$staged"
  rm -f -- "$work/archive.tar.gz" "$work/SHA256SUMS" "$work/n4h"
  rmdir -- "$work" 2>/dev/null || true
}
trap cleanup EXIT
asset=n4h-x86_64-unknown-linux-gnu.tar.gz
base=https://github.com/tomiok/n4h/releases
if [[ "$version" == latest ]]; then
  # Resolve once so the archive and checksum cannot come from different releases
  # if a new latest release is published between the two downloads.
  resolved=$(curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL -o /dev/null -w '%{url_effective}' "$base/latest") || die 'No published release found. See README for a source install.'
  version=${resolved##*/}
  [[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]] || die 'Could not resolve a release tag.'
fi
download=$base/download/$version
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL "$download/$asset" -o "$work/archive.tar.gz" || die "Release $version has no supported binary yet."
curl --proto '=https' --proto-redir '=https' --tlsv1.2 -fsSL "$download/SHA256SUMS" -o "$work/SHA256SUMS" || die 'Could not download checksums.'
expected=$(awk -v name="$asset" '$2 == name { print $1; count++ } END { if (count != 1) exit 1 }' "$work/SHA256SUMS") || die 'Checksum manifest must identify exactly one matching asset.'
[[ "$expected" =~ ^[0-9a-f]{64}$ ]] || die 'Invalid SHA-256 checksum.'
actual=$(sha256sum "$work/archive.tar.gz"); actual=${actual%% *}
[[ "$actual" == "$expected" ]] || die 'Checksum mismatch; nothing installed.'
# Extract only the fixed binary member, never arbitrary release paths.
tar -xzf "$work/archive.tar.gz" -C "$work" -- n4h || die 'Invalid release archive.'
[[ -f "$work/n4h" && ! -L "$work/n4h" ]] || die 'Archive binary is not a regular file.'
chmod 755 "$work/n4h"
actual_version=$("$work/n4h" --version) || die 'Release binary cannot run on this system.'
[[ "$actual_version" == "n4h ${version#v}" ]] || die 'Binary version does not match the selected release.'
safe_path "$bin_dir/n4h"
mkdir -p -- "$bin_dir"
staged=$(mktemp "$bin_dir/.n4h-install.XXXXXXXX")
cp -- "$work/n4h" "$staged"
chmod 755 "$staged"
if [[ -e "$bin_dir/n4h" ]]; then
  ((replace)) && [[ -f "$bin_dir/n4h" && ! -L "$bin_dir/n4h" ]] || die 'Destination appeared or changed; refusing replacement.'
  mv -fT -- "$staged" "$bin_dir/n4h"
else
  # Hard-link publication is atomic and cannot clobber an existing destination.
  ln -- "$staged" "$bin_dir/n4h" || die 'Destination appeared; refusing replacement.'
  rm -f -- "$staged"
fi
staged=
printf 'n4h: installed %s at %s/n4h\n' "$version" "$bin_dir" >&2
export PATH="$bin_dir:$PATH"
if ((!cli_only)); then
  setup_args=(setup)
  [[ -z "$harness" ]] || setup_args+=(--harness "$harness")
  [[ -z "$scope" ]] || setup_args+=(--scope "$scope")
  ((!yes)) || setup_args+=(--yes)
  "$bin_dir/n4h" "${setup_args[@]}" || die "Binary is installed, but skill setup did not complete. Retry: $bin_dir/n4h setup"
fi
"$bin_dir/n4h" doctor
printf 'n4h: ensure this directory is on the PATH used by your shell AND harness: %s\n' "$bin_dir" >&2
printf 'n4h: no shell profile was edited. In your next shell, check: n4h doctor\n' >&2
printf 'n4h: for Bash, add the directory to this shell with: export PATH=%q:"$PATH"\n' "$bin_dir" >&2
}

main "$@"
