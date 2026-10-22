#!/usr/bin/env bash
set -euo pipefail

macro_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$macro_root"
mkdir -p verification
if [[ "${1:-}" != --worker ]]; then
    set +e
    bash "$macro_root/scripts/bootstrap-macos.sh" --worker 2>&1 |
        tee "$macro_root/verification/build-macos-latest.log"
    macro_status=("${PIPESTATUS[@]}")
    if [[ "${macro_status[0]}" -ne 0 ]]; then exit "${macro_status[0]}"; fi
    exit "${macro_status[1]}"
fi

if [[ "$(uname -s)" != Darwin ]]; then
    echo "이 설치 스크립트는 macOS용입니다." >&2
    exit 2
fi
for macro_command in curl shasum node npm xcrun; do
    command -v "$macro_command" >/dev/null || {
        echo "필수 도구가 없습니다: $macro_command" >&2
        exit 2
    }
done
xcrun --find clang >/dev/null
xcrun --find swiftc >/dev/null

macro_tools="$macro_root/.build-tools"
mkdir -p "$macro_tools"
# These are Rust's actual configuration variables, scoped to this process and its
# children. No user shell profile, system toolchain, or permission is changed.
export CARGO_HOME="$macro_tools/cargo"
export RUSTUP_HOME="$macro_tools/rustup"
export PATH="$CARGO_HOME/bin:$PATH"

if [[ ! -x "$CARGO_HOME/bin/cargo" || ! -x "$CARGO_HOME/bin/rustup" || ! -x "$CARGO_HOME/bin/rustc" ]]; then
    case "$(uname -m)" in
        arm64) macro_host=aarch64-apple-darwin ;;
        x86_64) macro_host=x86_64-apple-darwin ;;
        *) echo "지원하지 않는 Mac CPU입니다." >&2; exit 2 ;;
    esac
    macro_download="$(mktemp -d "$macro_tools/rustup-download.XXXXXX")"
    trap 'rm -rf "$macro_download"' EXIT
    macro_installer_url="https://static.rust-lang.org/rustup/dist/$macro_host/rustup-init"
    echo "Rust 설치 도구와 공식 SHA-256 파일을 받습니다."
    curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --fail --silent --show-error --location --connect-timeout 10 \
        --max-time 300 "$macro_installer_url" -o "$macro_download/rustup-init"
    curl --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --fail --silent --show-error --location --connect-timeout 10 \
        --max-time 60 "$macro_installer_url.sha256" -o "$macro_download/rustup-init.sha256"
    macro_checksum="$(awk 'NR == 1 {print $1}' "$macro_download/rustup-init.sha256")"
    [[ "$macro_checksum" =~ ^[0-9a-fA-F]{64}$ ]] || {
        echo "공식 체크섬 형식을 확인할 수 없습니다." >&2
        exit 1
    }
    (
        cd "$macro_download"
        printf '%s  rustup-init\n' "$macro_checksum" | shasum -a 256 -c -
    )
    chmod u+x "$macro_download/rustup-init"
    "$macro_download/rustup-init" --default-host "$macro_host" \
        --default-toolchain stable --profile minimal --no-modify-path -y
fi

export RUSTUP_TOOLCHAIN=stable
if ! "$CARGO_HOME/bin/rustc" --version >/dev/null; then
    "$CARGO_HOME/bin/rustup" toolchain install stable --profile minimal
fi
"$CARGO_HOME/bin/rustc" --version
"$CARGO_HOME/bin/cargo" --version
echo "의존성 설치, 테스트, 앱·DMG 빌드를 시작합니다."
bash scripts/build-macos.sh
[[ -d "$macro_root/target/release/bundle/macos/Integrated Macro.app" ]] || {
    echo "빌드 명령은 끝났지만 앱 번들을 찾지 못했습니다." >&2
    exit 1
}
compgen -G "$macro_root/target/release/bundle/dmg/*.dmg" >/dev/null || {
    echo "빌드 명령은 끝났지만 DMG를 찾지 못했습니다." >&2
    exit 1
}
echo "빌드 완료: $macro_root/target/release/bundle"
