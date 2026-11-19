#!/bin/bash
# Builds the production backend in WSL from a clean snapshot of a commit (git archive), not from the working folder -
# it may hold unfinished edits from other sessions. Sources live on the Linux FS (~/dbsrc), the build in ~/dbtarget.
#   wsl -d Ubuntu -u root -- bash /mnt/<drive>/<root>/workspace/ops/deploy/scripts/wsl_build_clean.sh [commit]
# The repository is the workspace that contains this script (REPO can be set explicitly).
. "$HOME/.cargo/env"
set -o pipefail
REPO=${REPO:-$(cd "$(dirname "$0")/../../.." && pwd)}
REF=${1:-HEAD}
COMMIT=$(git -C "$REPO" -c safe.directory='*' rev-parse --short "$REF") || exit 1
echo "commit $COMMIT"
rm -rf "$HOME/dbsrc" && mkdir -p "$HOME/dbsrc"
# backend and content (the tests read content/seasons)
git -C "$REPO" -c safe.directory='*' archive "$REF" core/backend content | tar -x -C "$HOME/dbsrc" || exit 1
# git archive sets file times to the commit time: if the previous build is newer, cargo will not rebuild changed files
# (11.11: the previous commit's binary went to production). Set file times to "now".
find "$HOME/dbsrc" -type f -exec touch {} +
cd "$HOME/dbsrc/core/backend" || exit 1
export RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR="$HOME/dbtarget"
start=$(date +%s)
cargo test --release --locked -p dbserver-rs > "$HOME/dbtest.log" 2>&1
tcode=$?
grep -E "^test result|FAILED|panicked" "$HOME/dbtest.log" | tail -8
echo "tests exit=$tcode, $(( $(date +%s) - start )) s"
[ $tcode -ne 0 ] && exit $tcode
cargo build --release --locked -p dbserver-rs > "$HOME/dbbuild.log" 2>&1
code=$?
grep -E '^error|Finished' "$HOME/dbbuild.log" | tail -5
echo "build exit=$code, $(( $(date +%s) - start )) s"
echo "$COMMIT" > "$HOME/dbtarget/release/dbserver-rs.commit"
sha256sum "$HOME/dbtarget/release/dbserver-rs"
exit $code
