# macOS 네이티브 앱

SwiftUI/AppKit 프로그램과 별도 Swift native messaging host입니다. macOS 13+, Universal arm64/x86_64로 빌드하며 외부 Python/Node 설치 없이 앱을 실행합니다. 원본 Windows EXE를 Wine으로 실행하는 방식이 아닙니다.

설치는 루트 README의 Mac 절차를 따르세요. 앱의 `연결 · 기록` 탭에서 확장 폴더를 열고 연결을 등록합니다. 앱을 이동했다면 다시 등록합니다. 최초 실행은 연결 대기 상태이며 실제 예매에 사용할 좌석 프로필을 자동 추측하지 않습니다.

- 앱 내부: `Contents/MacOS/NolTicketAssistant`, `NolTicketNativeHost`
- 확장/연습: `Contents/Resources/extension`, `rehearsal-competition.html`
- 사용자 설정/IPC: `~/Library/Application Support/NOLTicketAssistant`
- Chrome 등록: `~/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nol_ticket_assistant.mac.json`
- Edge 등록: `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts/com.nol_ticket_assistant.mac.json`

등록 버튼을 누를 때만 브라우저의 등록 파일을 생성합니다. 기존 파일은 날짜가 붙은 백업으로 보존합니다. 제거하려면 위의 해당 등록 JSON과 앱을 삭제하세요. 별도로 저장한 사용자 설정은 유지됩니다.

IPC는 64KiB 이하 JSON, 4바이트 little endian 길이 프레임입니다. host는 ping/status와 정해진 앱 명령만 처리합니다. JSON은 0600으로 원자 교체하며 새 host는 이전 명령을 재실행하지 않습니다. Chrome/Edge 상태는 각각 status-chrome.json/status-msedge.json에 저장합니다. 앱은 6초 이내 heartbeat가 있어야 실행을 허용합니다.

개발자 빌드: Xcode Command Line Tools/Swift, Python3 필요. 프로젝트 루트에서 `macos/build.sh`. 결과 `macos/build/NOL Ticket Assistant.app`. ad-hoc 코드 서명은 포함하며 Apple 공증/유료 Developer ID 서명은 없습니다.

자가검사: 앱 또는 host 실행 파일에 `--self-test`. 실제 IPC 검사는 프로젝트 루트 `python3 tests/mac-host.test.py`이며 임시 폴더만 사용합니다. Intel 바이너리는 교차 빌드했으나 Intel Mac 런타임 검증은 하지 않았습니다.


v0.3: ‘구역 진입’ 탭에서 사용자 확인 패널, 단일 구역, 그 구역의 지도 로딩 조건을 지정합니다. ‘브라우저에서 구역 지정’은 요소/상대 위치를 저장하며 실제 예매 클릭을 막습니다. ‘좌석 조건 → 화면 픽셀 → 지도 요소 지정’은 지도 크기를 자동으로 읽어 검색 범위를 계산합니다. 상대 위치·구역 식별 정보는 앱 저장과 JSON 편집을 거쳐도 보존됩니다. Canvas의 구역 내부 확대/이동은 실제 구조 확인이 필요하며 자동 구역 지정은 지원하지 않습니다.
