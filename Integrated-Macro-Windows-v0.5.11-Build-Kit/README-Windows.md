# 통합 매크로 0.5.11 — Windows 빌드 준비 키트

이 ZIP은 **Windows 실행 파일·설치 프로그램이 아닌 빌드 준비 키트**입니다. 현재 Mac의 Windows Rust 대상·SDK·도구가 없고 도구 다운로드가 DNS 오류로 막혀, Windows 설치 파일을 아직 만들지 못했습니다.

`source/`의 앱 소스 138개는 Mac 배포본 0.5.11의 기록과 SHA-256이 전부 같습니다. 미배포 검색 조건 교정은 포함하지 않았습니다. 기존 F9·좌석 인식 동작도 그대로이며, 이 키트가 해당 문제를 해결한 버전은 아닙니다. 개인 설정·예약 정보·로그인 정보는 포함하지 않습니다.

## Windows PC에서 빌드

Windows x64에서 다음 도구를 준비합니다. 개발 도구는 빌드할 PC에만 필요합니다.

- Node.js 24와 npm
- Rust의 MSVC 도구 체인과 `x86_64-pc-windows-msvc` 대상
- Visual Studio Build Tools의 C++ 데스크톱 개발 구성과 Windows SDK
- 의존성·NSIS 도구 다운로드가 가능한 인터넷 연결

설치 조건은 [Tauri Windows 개발 준비](https://v2.tauri.app/start/prerequisites/#windows)를 참고하세요. 압축을 푼 뒤 `Build-Windows.cmd`를 실행합니다. 스크립트는 시스템 정책을 변경하거나 개발 도구를 자동 설치하지 않습니다. PowerShell 실행 정책 때문에 거부되면 허용된 실행 환경에서 스크립트를 실행하세요.

빌드 스크립트는 원본·시험 입력 해시를 확인하고 작업용 복사본에서 고정된 의존성 설치, JavaScript·Rust 검사, x64 NSIS 설치 파일 생성을 순서대로 수행합니다. 실패한 검사는 건너뛰지 않습니다. 각 실행 결과는 `output/<실행 번호>/`의 `report.json`, `build.log`, 성공 시 `artifacts/`에 기록됩니다. 생성된 설치 파일에 대한 Windows 실행 검증은 별도로 필요합니다.

앱 실행에는 WebView2 Runtime이 필요하며 기본 NSIS 설치 프로그램은 없으면 온라인 설치를 시도합니다. 현재 구성에는 Windows 서명 인증서가 없으며 생성 파일의 실제 서명 상태는 빌드 보고서에 기록됩니다. ([Tauri 설치 파일 안내](https://v2.tauri.app/distribute/windows-installer/))

## GitHub Actions에서 빌드

이 키트의 내용과 숨김 파일 `.gitattributes`, `.github/`를 사용할 저장소 루트에 넣으면 `.github/workflows/build-windows-0.5.11.yml`의 수동 실행 작업으로 Windows 빌드를 요청할 수 있습니다. `.gitattributes`는 Windows 체크아웃에서도 원본 해시에 맞는 줄바꿈을 보존합니다. 작업은 설치 파일을 artifact로 보관하며 GitHub Release에 자동 게시하지 않습니다. 이 키트에서는 실제 작업을 업로드하거나 실행하지 않았습니다.

## 구성과 검증 범위

- `source-manifest.json`: 배포 0.5.11의 앱 소스 해시
- `fixture-manifest.json`: 기존 합성 시험 입력 417개 해시
- `verification/`: 이 준비 작업의 검사·리뷰 기록

동일 소스·시험 입력의 무결성과 Windows 실행 성공은 별개입니다. 현재 준비 키트의 PowerShell/CMD도 Windows에서 아직 실행하지 않았습니다. Windows 설치·브라우저 연결·F8 채집·예약 클릭·F9·좌석 선택의 실기기 검증은 남아 있습니다.
