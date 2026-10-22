# 0.5.11 macOS 배포 검증

2026-10-22 사용자 요청에 따라 F9 수정과 직전 실행 명령 감사를 포함해 배포했습니다.
배포 폴더와 ZIP은 작업공간의 `Integrated-Macro-macOS-v0.5.11/`와 `Integrated-Macro-macOS-v0.5.11.zip`입니다.
Apple Silicon(arm64), macOS 13 이상용이며 로컬 ad-hoc 서명입니다. Intel 앱·Windows 빌드·Apple 공증은 포함하지 않습니다.

## 포함 동작

F9의 실행 대기·중지·예약 대기·미전달 입장 제한을 없앴습니다. 사용자의 직접 완료 신호로 이전 미전달 입장 순서를 폐기하고 구역·좌석 관측으로 넘어갑니다.
유지 좌석·미해결 오류가 없으면 구역 우선 ON에서 구역, OFF에서 좌석부터 진행합니다.
같은 창의 좌석·알려진 오류 이력을 유지하며 F4 뒤 F9도 이어갑니다. 다른 ID/PID 창은 새 문맥으로 초기화합니다.
같은 창 재연결은 새 캡처 화면 번호로 비교를 재개합니다. 실패한 네이티브 연결 뒤 재연결은 새 문맥으로 초기화될 수 있습니다.

직전 미배포 수정인 기본 OFF **날짜·회차·예매 버튼 직접 진행**, F1/F2/F5·UI 시작 명령의 재시도와 지연 응답 무효화도 포함합니다.
옵션·저장 경로·앱 식별자와 기존 배포는 보존했습니다. F9는 모든 사이트 화면을 자동 분류하지 않으며 창 연결·권한·색상·탐색 범위 가드는 필요합니다.

## 검토와 소스 연속성

- [F9 교차 검토](../f9-any-state/README.md), [최종 감사 manifest](../f9-any-state/final-manifest.json), [리뷰 해시 연결](../f9-any-state/review-source-continuity.json).
- 직전 [실행 명령 감사](../reentrant-commands/README.md)의 최종 136파일과 이번 시작 baseline이 일치합니다. 최종 소스는 138파일이며 [변경 목록](source-continuity.json)에 기록했습니다.
- [버전 변경](version-only.diff)은 자체 버전 9파일에 한정됩니다. 의존성 버전·앱 식별자는 바꾸지 않았습니다.
- [빌드 전](source-before-build.json)과 [빌드 후](source-after-build.json)의 138파일 해시가 동일하고 독립 리뷰의 최종 해시와 일치합니다.
- 초기 전체 검사도 통과했으나 F4→F9 오류창 기록 결함을 추가 발견해 [그 후보를 미배포로 보존](preliminary-stop-conflict/status.json)했습니다. 교정·재검토 후 전체 게이트를 다시 실행했으며 최종 실행파일 해시는 예비 후보와 다릅니다.

## 실행한 검사

로컬 Rust 도구체인으로 `node scripts/check.mjs --native --build`를 실행했습니다.

- JavaScript 172개 통과, Rust 466개 통과·실패 0개·수동 검사 5개 제외.
- 독립 Engine 25개·실제 DOM/지연 IPC 19개·후속 Desktop 132개는 위 검사의 부분 범위로 별도 합산하지 않습니다.
- Swift arm64/x86_64 컴파일·호스트 합성 ABI, JavaScript 구문, 소스 안정성 게이트, 오프라인 lock 고정 최적화 앱 빌드와 서명 검사 통과.
- 빌드 앱과 배포 앱의 내용·권한 동일, ZIP 재추출 후 파일·권한·앱 서명 동일.
- 이전 0.5.10 앱·ZIP과 AGENTS 등 22개 보호 파일 및 직전 감사 증거는 변경되지 않았습니다.

[전체 로그](build-and-checks.log), [구조화 검사](local-checks.json), [빌드 결과](build-result.json),
[패키지·ZIP 영수증](distribution-receipt.json), [독립 앱 검사](independent-artifact-review.json), [반대 패키지 검사](adversarial-package-review.json)를 보존했습니다.

ZIP SHA-256: `f8430ad8262923840ab549c6fd25d128dc5e7dad44fbfb91e483cfdc0839722c` · 5,046,088 bytes.

## 실제 실행과 한계

앱 실행·물리 F9·실제 브라우저 입력/캡처/오버레이·로그인 사이트 예매·Windows·전체 지연은 미검증입니다.
화면 변화 감지는 휴리스틱이며 좌석 확보나 서버 선점 성공을 확정하지 않습니다. 지원하지 않는 표시나 불명확한 수량에서는 관측 보류가 남습니다.
이미 전달된 OS 입력과 이미 시작한 저장 commit은 철회하지 않습니다. 사용자 설정·권한은 변경하지 않았습니다.

검사 보고서의 `releaseReady:false`는 실제 운영 흐름을 검증하지 않았다는 뜻으로 유지했습니다. 로컬 빌드·패키징 검사를 실패로 바꾸거나 실제 예매 성공으로 해석하지 않습니다.
빌드에는 미사용 `Request.received_at`·`VisualRecovery.status` 경고와 합성 네이티브 검사의 macOS 서비스 메시지가 있었으며 각 명령은 성공 종료했습니다.
