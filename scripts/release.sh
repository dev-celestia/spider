#!/usr/bin/env bash
# One-command release to crates.io: bump version, test, commit, publish, tag, push.
#
# Usage:
#   ./scripts/release.sh [patch|minor|major|X.Y.Z]
#
# Defaults to "patch" if no argument is given. Requires a clean git tree.
set -euo pipefail

cd "$(dirname "$0")/.."

BUMP="${1:-patch}"
CRATE=$(sed -n 's/^name = "\(.*\)"/\1/p' Cargo.toml | head -n1)
CURRENT=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)

# --- Preflight -----------------------------------------------------------
if [[ -n "$(git status --porcelain)" ]]; then
  echo "Error: working tree is not clean. Commit or stash your changes first." >&2
  git status --short >&2
  exit 1
fi

# --- Compute the new version ---------------------------------------------
if [[ "$BUMP" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  NEW="$BUMP"
else
  IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"
  case "$BUMP" in
    major) NEW="$((MAJOR + 1)).0.0" ;;
    minor) NEW="$MAJOR.$((MINOR + 1)).0" ;;
    patch) NEW="$MAJOR.$MINOR.$((PATCH + 1))" ;;
    *) echo "Invalid bump: '$BUMP' (use patch, minor, major, or X.Y.Z)" >&2; exit 1 ;;
  esac
fi

echo "==> Releasing $CRATE $CURRENT -> $NEW"

# --- Bump Cargo.toml and refresh Cargo.lock -------------------------------
if [[ "$(uname)" == "Darwin" ]]; then
  sed -i '' "0,/^version = \".*\"/s//version = \"$NEW\"/" Cargo.toml
else
  sed -i "0,/^version = \".*\"/s//version = \"$NEW\"/" Cargo.toml
fi
cargo update -p "$CRATE" --quiet

# --- Verify before publishing ---------------------------------------------
echo "==> Running tests..."
cargo test --quiet

echo "==> Dry-run publish..."
cargo publish --dry-run --quiet

# --- Commit the bump (crates.io refuses dirty trees) -----------------------
read -rp "Publish $CRATE $NEW to crates.io? [y/N] " CONFIRM
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
  echo "Aborted. Revert the bump with: git checkout Cargo.toml Cargo.lock"
  exit 1
fi

git add Cargo.toml Cargo.lock
git commit -m "chore(release): v$NEW"

# --- Publish ---------------------------------------------------------------
echo "==> Publishing..."
cargo publish

git tag "v$NEW"

if git push origin HEAD && git push origin "v$NEW"; then
  echo "==> Done: $CRATE $NEW published, tagged and pushed."
else
  echo "==> Published $NEW, but push failed. Run manually:"
  echo "    git push origin HEAD && git push origin v$NEW"
fi
