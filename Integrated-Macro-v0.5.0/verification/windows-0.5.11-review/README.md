# Windows 0.5.11 배포 준비 기록

현재 산출물은 Windows 실행 파일이 아닌 **빌드 준비 키트**입니다. 실제 Windows 설치 파일 배포는 완료되지 않았습니다.

## 기준과 보존

Mac 배포 0.5.11의 source-after-build.json에 연결된 138개 파일을 정확히 복원했습니다. 미배포 calibration-retry 교정은 포함하지 않았습니다. 테스트에 필요한 기존 합성 입력 417개도 경로와 해시를 보존했습니다. 원본 개발 소스와 보호 대상 Mac 배포 파일·AGENTS.md 22개는 변경하지 않았습니다. 실행 중인 앱·설정·예약을 제어하지 않았습니다.

## 확인한 결과

- 분리한 원본으로 macOS arm64에서 JavaScript 172개, Rust 466개 통과, 수동 검사 5개 제외. Windows 런타임 검증이 아닙니다.
- Windows 대상 cargo check --locked --offline 시도는 E0463(Windows 대상 표준 라이브러리 부재), exit 101로 중단됐습니다.
- Windows target/SDK/교차 빌드 도구가 없으며 도구 다운로드는 DNS 오류로 실패했습니다. Windows VM/원격 실행 환경도 확인되지 않았습니다.
- 별도 PS/CMD·수동 GitHub Actions 워크플로를 만들고 독립·적대 리뷰로 입력 corpus 누락, Git 줄바꿈 변환, VS 환경 초기화, 부분 산출물 노출 문제를 교정했습니다.
- PowerShell/CMD 및 GitHub Actions는 아직 실행하지 않았습니다. Windows PE·설치·WebView2·입력·브라우저 연결·실제 예매 검증은 남아 있습니다. 기존 F9/인식 제한도 그대로입니다.

## 산출물

상위 폴더의 Integrated-Macro-Windows-v0.5.11-Build-Kit 및 동일 이름 ZIP. 정확한 크기·SHA·검사 범위는 build-kit-receipt.json과 패키지 BUILD-STATUS.json을 확인하세요. ZIP에는 개인 설정·로그인 정보·실사용 runtime 로그나 node_modules/target를 넣지 않았습니다.

최종 배포를 이어가려면 Windows 빌드 도구를 사용할 PC 또는 사용할 GitHub Actions 저장소가 필요합니다. 사용자에게 환경을 질문했으며 아직 답변을 받지 않았습니다.
