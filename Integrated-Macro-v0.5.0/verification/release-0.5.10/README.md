# 0.5.10 macOS 빌드 기록

2026-10-21 사용자 요청으로 검증 소스를 패키징했습니다. 앱과 ZIP 생성, 서명, 압축 재추출 검사를 완료했습니다.
배포 위치는 작업공간의 `Integrated-Macro-macOS-v0.5.10/`와 `Integrated-Macro-macOS-v0.5.10.zip`입니다.
Apple Silicon(arm64), macOS 13 이상용이며 로컬 ad-hoc 서명입니다. Apple 공증 및 Intel 앱 배포는 포함하지 않습니다.

## 포함된 수정과 소스 연속성

직전 선택 수량 감사의 131개 파일 중 9개 파일에서 자체 버전·UI 표시만 0.5.10으로 바꾸고 README의 현재/과거 배포 안내를 정리했습니다.
기능 코드·의존성·앱 식별자·권한 설정은 그대로입니다. 좌석 외곽·연석 우선·선택 수량 확인의 기존 검증 수정사항을 포함합니다.
세 기능은 기본 OFF이며 저장된 설정은 사용자가 확인해야 합니다.

- [버전 변경과 상호 리뷰](version-update/README.md)
- [독립 소스 검토](independent-review-source.json)
- [반대 검토](adversarial-review.md)와 [독립 ZIP 검사](adversarial-review.package.json)
- [내장 UI·서명·실행파일 검토](independent-review-artifact.json)와 [배포 앱 비교](independent-review-distribution.json)
- [빌드 전 소스](source-before-build.json)와 [빌드 후 소스](source-after-build.json): 131개 동일
- [검증 입력 연속성](fixture-continuity.json): 484개 파일 해시 일치(테스트 개수 아님)

## 실행한 검사

`node scripts/check.mjs --native --build`를 저장소의 Rust 도구체인으로 실행했습니다.

- JavaScript 150개 통과, Rust 425개 통과·실패 0개·수동 검사 5개 제외.
- JavaScript 구문, Swift arm64/x86_64 컴파일 및 호스트 합성 ABI 검사 통과.
- 소스 안정성 게이트, 오프라인/lock 고정 Tauri 최적화 빌드, 앱 서명 검사 통과.
- 빌드 앱과 배포 앱 동일, ZIP 재추출 후 모든 파일 내용·권한 동일, 추출 앱 서명 검사 통과.
- 이전 0.5.9 배포와 AGENTS 등 67개 보호 파일은 그대로입니다.

[전체 검사 로그](build-and-checks.log), [구조화 검사](local-checks.json), [빌드 결과](build-result.json),
[패키지·ZIP 해시와 무결성 기록](distribution-receipt.json)에 근거를 남겼습니다.

## 실제 실행과 한계

앱을 실행하거나 마우스·키보드를 보내지 않았고 사용자 설정·권한을 바꾸지 않았습니다.
실제 예매창의 0→1→2석 전환, Windows 빌드/실행, 서버 선점·결제 성공과 전체 반응 시간은 미검증입니다.
수량 인식은 지원 글꼴·배치에 제한되며 모호하면 입력을 보류합니다. 직전 큰 합성 화면 판독은 약 240–260ms였습니다.
자세한 범위는 [선택 수량 감사](../selection-count-audit/README.md)에 있습니다.

빌드 중 기존 미사용 `VisualRecovery.status` 경고와 네이티브 합성 검사 프로세스의 macOS 서비스 연결 메시지가 있었습니다.
각 명령은 성공으로 종료됐지만 이를 실제 OS 입력·캡처 성공의 근거로 사용하지 않습니다.

두 리뷰어가 최종 산출물과 상대의 검사 방법을 확인했으며 추가 패키징 차단 결함은 발견하지 못했습니다.
이 결론은 위 실제 실행·인식 지원 범위의 한계를 해소했다는 뜻은 아닙니다.
