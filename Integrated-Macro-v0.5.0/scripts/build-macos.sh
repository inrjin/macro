#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
macro_tools="$(pwd)/.build-tools"
if [[ -x "$macro_tools/cargo/bin/cargo" ]]; then
    export CARGO_HOME="$macro_tools/cargo"
    export RUSTUP_HOME="$macro_tools/rustup"
    export RUSTUP_TOOLCHAIN=stable
    export PATH="$CARGO_HOME/bin:$PATH"
fi
command -v cargo >/dev/null || { echo "Rust/Cargo가 필요합니다. README의 빌드 준비를 확인하세요." >&2; exit 2; }
command -v npm >/dev/null || { echo "Node.js/npm이 필요합니다." >&2; exit 2; }
xcrun --find swiftc >/dev/null
if [[ -f package-lock.json ]]; then
    npm ci --cache "$macro_tools/npm-cache" --no-audit --no-fund
else
    npm install --cache "$macro_tools/npm-cache" --no-audit --no-fund
fi
npm test
if [[ -f Cargo.lock ]]; then
    cargo test --workspace --locked
else
    cargo test --workspace
fi
npm run build -- --bundles app,dmg

