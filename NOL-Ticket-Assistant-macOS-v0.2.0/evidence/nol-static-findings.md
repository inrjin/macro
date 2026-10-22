NOL 상품 공개 코드 정적 분석 기록

대상: https://nol.yanolja.com/ticket/products/26010352 — 저장된 `product.html` 및 그 페이지가 참조한 JS 23개. 공개 자산 URL 목록은 `script-urls.json`, 빌드 경로는 `nol-ticket-web/f6c471`이다. 2026-10-18 작업 중 로컬 증거만 읽었다. 대상 JS를 실행하지 않았고 인증·예매 API를 호출하지 않았으며 우회 구현은 하지 않았다.

확신 표기: **관찰**은 로컬 바이트/코드 구조에서 직접 확인, **추론**은 그 코드가 해당 UI 경로에서 실행된다고 가정한 해석, **미확인**은 별도 실행 증거가 필요한 부분이다. 바이트 위치는 원본 파일 UTF-8 **0 기반, 끝 제외 [start,end)** 이다. 축약 코드 변수와 번들 파일명은 이 빌드에만 유효하다.

1. 상품 스냅샷과 조회 범위

- **관찰:** HTML title은 `YUURI LIVE 2026 IN SEOUL`, 공연 기간 필드는 `2026-12-05`–`2026-12-06`이다. 상품 `26010352`의 `goodsStatus`는 `Y`, 일반 판매 코드 `21001`, `bookingOpenTime`은 `2026-07-31 20:00:00`, `bookingEndTime`은 `2026-12-05 11:00:59`, `presales`/`presalesTimes`는 빈 배열이다. [E01] [E02] [E03] [E04]
- **관찰:** enum에서 `Y=Available`, `N=NotAvailable`, `C=Cancelled`, `F=SoldOut`, `D=Display`로 정의한다. `Y`는 실제 잔여 좌석이 존재한다는 증거가 아니다. [E06]
- **관찰:** 저장 HTML에는 `#booking-aside-container`와 로딩 `role="status"`, disabled 예매 버튼이 렌더되어 있다. 날짜/회차가 로드된 브라우저 DOM 스냅샷은 아니다. [E05] HTML byte [69200,73300).
- **미확인:** 실제 회차 목록, 현재 잔여석, 로그인 후 최종 화면, 사용자의 대기열 상태·보안 확인·좌석 선택 화면은 이 증거에 없다. 부모의 headless 실행은 서비스 일시불가였으므로 정상 실행 관측으로 취급하지 않는다.

2. 예매 진입 버튼 구분

- **관찰:** 데스크톱 실제 예매 UI는 `#booking-aside-container` 내부이다. 상단 고정 네비게이션의 별도 `예매하기` 버튼은 `eO()`를 호출해 이 컨테이너 위치로 `window.scrollTo({...,behavior:"smooth"})`만 수행한다. 화면 전체에서 텍스트만 검색해 첫 버튼을 고르면 다른 동작이 될 수 있다. [E08] [E07] main byte [1781020,1782360).
- **관찰:** 데스크톱 실제 버튼의 `onClick`은 먼저 `ensureAuthenticated`를 호출하고, 날짜 선택 모드면 선택된 `playSeq`가 현재 schedules 안에 있는지 확인한 뒤 `navigateToSso`를 호출한다. Display 상품은 `playSeq:null`, 일반 날짜 선택 상품은 선택된 `playSeq`, 그 외 모드는 null로 진입한다. [E10] main byte [1728060,1729580).
- **관찰:** 데스크톱 disabled 조건은 lottery closed, 판매종료/취소 kind, 인증 조회중, 유효 날짜 없음, 그리고 로그인된 날짜 선택 모드에서 선택 회차 없음/인증검증중/일정조회중 등이다. **`open-scheduled-*` 자체가 disabled 조건은 아니다.** [E09]
- **관찰:** 모바일 하단 최초 CTA는 비로그인이면 로그인 이동, 본인인증이 없으면 인증을 시작한다. 일반 Available 상품이고 일반 판매 또는 선예매 시간 범위 안이면 날짜 선택 모달을 연다. 다른 경로는 인증 재확인 후 SSO로 이동할 수 있다. [E11] main byte [280220,282190).
- **관찰:** 모바일 모달의 확정 버튼은 `data-testid="modal-booking-button"`, 텍스트 `예매하기`, disabled 조건 `!selectedPlaySeq || isVerifyingAuth`이다. 모달 hash 상수는 `booking_date_time_picker`, 헤더는 `날짜 및 회차 선택`이다. [E12] [E13] main byte [291500,292030).
- **추론:** enabled `예매하기`만으로 판매가 열렸거나 좌석을 확보할 수 있다고 판단할 수 없다. 정상 UI 클릭 후 실제 도착 페이지에서 준비 상태를 별도로 확인해야 한다.

3. 날짜·회차와 좌석 수 표시

- **관찰:** `BookingDayPicker`는 schedules를 `playDate`로 묶는다. 날짜 선택 시 해당 날짜의 회차가 하나이면 그 회차를 자동 선택하고, 여러 개면 `selectedPlaySeq=null`로 바꾸어 사용자의 회차 선택을 기다린다. 최초 표시 때도 회차가 하나면 effect로 자동 선택한다. [E14] main byte [273600,275760).
- **관찰:** 달력은 `react-calendar`, locale `ko-KR`, 월 표시는 `yyyy.MM`, 날짜의 표시 숫자는 `d`이다. 일정에 없는 날짜는 `tileDisabled`로 비활성화한다. [E15] [E17]
- **관찰:** 날짜 버튼 안에는 `abbr[aria-label]`이 있으며 label은 locale의 긴 날짜 포맷이다. `2026년 12월 5일` 같은 정확한 문자열은 실제 DOM에서 확인해야 한다. 월 이동 버튼은 `.react-calendar__navigation__next-button` / `__prev-button`, 선택 날짜는 `.react-calendar__tile--active`이다. [E16] main byte [227650,227810), [231200,232430).
- **관찰:** 회차 버튼은 `button[data-is-selected]`이고 내부 `time[datetime]`에 `playTime`을 표시한다. 클릭은 `onShowtimeSelect(playSeq)`만 수행한다. CSS class보다 `time` 값과 선택 속성이 표식으로 유용하다. [E18] [E19]
- **관찰:** `remainCnt`는 좌석 등급별 수량 표시용이다. 검토한 회차 렌더 함수에서는 수량 0을 이유로 회차 버튼을 disabled 처리하지 않는다. 상품 속성에 따라 수량 또는 잔여석 정보를 감추고 `잔여석이 안내되지 않는 상품이에요.`를 표시한다. [E36] main byte [264100,268990).
- **관찰:** 일정 조회·회차 필터는 `saleOpenTime`/`saleCloseTime` 또는 활성 선예매 시간 범위를 검사하며 배송 예약 일수도 반영한다. 이 함수가 내부 조회 URL을 갖고 있다는 것은 코드 관찰일 뿐, 이 분석에서 직접 호출하지 않았다. [E23] main byte [213640,216960).

4. 오픈 시간·타이머의 실제 의미

- **관찰:** 날짜 비교는 `DateTime.now()`와 문자열 `fromFormat(...,"yyyy-MM-dd HH:mm:ss")`로 수행한다. 초기화 번들은 Luxon 기본 시간대를 `Asia/Seoul`로 설정한다. [E20] [E21]
- **관찰:** 데스크톱 kind는 종료/취소/추첨/패키지/상시 판매 등을 먼저 분기하고, Display 또는 가장 이른 선예매/일반 판매 오픈 시각 전이면 `open-scheduled-presale` 혹은 `open-scheduled-notice`로 표시한다. 메시지는 `오픈 예정`, `공지된 일정에 맞춰 예매를 시작할 수 있어요.`이다. main byte [1726170,1726560), [1733300,1733980), [1736260,1737280). [E24]
- **관찰:** `useLotteryPhase`에는 1초마다 `DateTime.now()`를 갱신하는 interval이 있다. 이 interval은 lottery 속성 상품에서만 동작한다. 일반 상품의 오픈 시각 countdown이나 일반 버튼 enable 타이머로 해석하면 안 된다. [E22]
- **한정된 부정 관찰:** 저장 JS 23개를 `server.?time`, `time.?offset`, `count.?down`, `now.?query`, `useNow`, `useCurrentTime`, `setInterval`, `refetchInterval` 패턴으로 검색했다. 예매 진입/일정 모듈에서 서버시각 동기화나 일반 판매 countdown hook은 찾지 못했다. `ServerTime` 문자열은 별도 AWS SDK 시스템 시계 보정 코드, `refetchInterval`은 공통 query 라이브러리 구현에서 나왔다. 이들은 예매 오픈 타이머를 입증하지 않는다.
- **추론/미확인:** 일반 버튼 상태는 React 렌더 당시 시각에 의존한다. 오픈 순간 자동으로 다시 렌더되어 활성화되는지, 동적 추가 번들/서버 응답이 이를 보완하는지는 미확인이다. 로컬 시각 비교를 서버의 최종 허용 판단으로 간주하면 안 된다.

5. 로그인·인증·이동

- **관찰:** 인증 조회 상태에는 `isLoggedIn`, `isIdentityValid` 등이 있고, 초기 query 캐시와 클릭시 재검증 mutation이 별도로 존재한다. `ensureAuthenticated`는 로그인·본인인증이 모두 유효해야 success를 반환한다. [E26] [E32]
- **관찰:** 로그인은 `accounts.yanolja.com`으로 현재 주소를 redirect 값에 넣어 현재 탭 이동한다. 본인인증은 웹에서 `authentication-window` 팝업이 열릴 수 있고, 팝업이 막히면 `본인 인증 하기` 대화상자로 재시도를 제공한다. 앱에서는 native bridge 분기가 있다. [E25] [E27] [E31]
- **관찰:** 실제 예매 진입 `navigateToSso`의 웹 분기는 `sso.yanolja.com`으로 **현재 탭**을 이동한다. 생성되는 SSO 경로는 `/sso/v1/bridge/token`, redirect 목적은 `tickets.interpark.com/gates/partner`이고 상품/공연장/선택 회차 등의 문맥을 전달한다. 이 기록은 정식 버튼이 호출하는 코드 구조 설명이며, 직접 링크/API 진입 구현 지침이 아니다. [E28] [E29] [E30] [E31]
- **미확인:** 이 경로 뒤에서 gate가 어떤 팝업/대기열/좌석 페이지를 실제로 여는지 확인하지 못했다. 상품 페이지의 `window.open` 존재만으로 “예매 버튼은 팝업”이라고 일반화하면 안 된다.
- **관찰:** 날짜·회차 초기 선택 문맥은 `sessionStorage`의 `bookingInitialSelection`에서 읽고 지우는 코드가 있다. 로그인/페이지 재방문 뒤 어떤 경로가 그 값을 기록하는지 전체 도달 가능성은 미확인이다. [E33]

6. DOM 표식 후보 (정적 관찰, 실행 검증 필요)

| 목적 | 권장 범위/표식 | 제약 |
|---|---|---|
| 데스크톱 예매 UI | `#booking-aside-container` | HTML에도 실제 존재. 내부 visible 버튼 중 정확 텍스트 `예매하기`와 disabled 상태를 확인 |
| 모바일 날짜 선택 확정 | `[data-testid="modal-booking-button"]` | 모달을 연 이후에 생성. 선택 회차가 없으면 disabled |
| 날짜 | `.react-calendar__tile`의 `abbr[aria-label]` | 날짜 숫자만으로 선택하면 인접 월 중복 가능. 조상 button의 disabled도 확인 |
| 선택된 날짜 | `.react-calendar__tile--active` | CSS 시각표식이라 빌드 변경 가능 |
| 월 제목/이동 | `.react-calendar__navigation__label__labelText`, `.react-calendar__navigation__next-button`, `.react-calendar__navigation__prev-button` | 이 페이지는 아이콘 label이라 accessible name이 비어 있을 수 있음 |
| 회차 | `button[data-is-selected]` 안 `time[datetime]` | 실제 visible 컨테이너 범위로 제한. 버튼 선택 상태 `data-is-selected="true"` 확인 |
| 로딩 | 예매 컨테이너의 `[role="status"]` | 저장 HTML은 이 상태. 날짜/텍스트가 아직 없다고 실패로 단정하지 않음 |
| 빈 일정 | 정확 텍스트 `예매할 수 있는 날짜가 없어요.` | 품절 자체와 일정 조회 오류를 구분 [E34] |
| 조회 실패 | `회차 정보를 조회하는중에 에러가 발생했습니다.`, `다시시도` | 오류 재시도 버튼은 예매 진입 버튼이 아님 |
| 인증 오류 | `일시적으로 에러가 발생했습니다. 다시 시도해 주세요.` | 예매 완료/준비 완료로 오인 금지 [E35] |

상품 페이지 이후 ready URL/selector는 이 코드만으로 제공할 수 없다. Windows에서 실제 로그인 후 정식 흐름을 관찰하고 사용자가 준비 완료 화면의 표식을 지정하는 방식이 필요하다.

7. REA 근거와 제한

- 분석: `analyze_javascript_application(input_path=.../evidence/scripts, source_paths=["0aikkt2ouk932.js"], detail="summary")`.
- parent Evidence: `ev_3ed49720e8ddc7c4c913db9652d147792d86249ecfc3ba60b0358312570977cf`.
- summary receipt: `ev_d6b18cecbdf012e7bd7c5ab44727ec0dece4a1f73501056161622fc8df478ee3`.
- targeted module references: `ev_8cf33ddd1516e1f55ea24e18a1a85cd32f17342fd8f2b2b697b6fc5ef387b359`.
- targeted module item: `ev_86e6bd82c5b2b1b8b2891ce52d7221b57acac0e47bc336eb764a3da755440bf6`.
- root inventory SHA-256: `9fb538bcf08d3e90ec9148ca7b9b2e5d5cd16005d877cd552f480d339d3e0610`.
- 23개 inventory 중 1개(1,792,260 byte)를 REA AST 분석했다. parse failure는 0. 나머지 22개는 REA source selection 밖이며 관계는 unknown이다. 본 보고서는 필요한 enum/auth/timezone 번들의 좁은 원문 구간을 별도 수동 정적 검토했다.
- semantic graph는 20,000 node/unknown ceiling 때문에 partial/truncated이며 20,264 semantic unknown이 남았다. 동적 호출/객체 흐름은 미확정이다. REA 결과에 브라우저 실행 증거는 없다.
- 아래 byte 근거는 원문 검색·독해의 출처다. REA가 모든 기능 관계를 자동 복원했다는 의미는 아니다.

8. 파일 식별

| 파일 | bytes | SHA-256 |
|---|---:|---|
| `scripts/0aikkt2ouk932.js` | 1792260 | `2f73c2274e80340c4fb012c4494004a2f4de24db4a728378843a3412a2e69377` |
| `scripts/17erxnd~wt7ql.js` | 659382 | `e85de8c9e1c08dffb598635ed02729a415313d459ae319edaba00f56ff1c9ce2` |
| `scripts/0ofov-x9w67.l.js` | 16079 | `62e786e127bc890ea6f201bfac1cc7dfc5136e1903f83ffabeb2e0c3f85c3bd2` |
| `scripts/134b7-pzle6r0.js` | 40045 | `0f090e8b4b5e6d6d593925599949e21ffa52bc86d2da72c52203077b76a832f1` |
| `product.html` | 211662 | `0d29554d7e1e68697b91154dc0415331e0aae2965f2ae0ed67e8f3a894962d59` |

9. 좁은 원문 근거

| ID | 파일 | UTF-8 byte [start,end) | 정확한 짧은 조각 |
|---|---|---:|---|
| E01 | `product.html` | [129343,129360) | `goodsStatus\":\"Y` |
| E02 | `product.html` | [129164,129203) | `bookingOpenTime\":\"2026-07-31 20:00:00` |
| E03 | `product.html` | [129208,129246) | `bookingEndTime\":\"2026-12-05 11:00:59` |
| E04 | `product.html` | [178971,179005) | `presales\":[],\"presalesTimes\":[]` |
| E05 | `product.html` | [69238,69266) | `id="booking-aside-container"` |
| E06 | `scripts/0ofov-x9w67.l.js` | [509,585) | `Available="Y",n.NotAvailable="N",n.Cancelled="C",n.SoldOut="F",n.Display="D"` |
| E07 | `scripts/0aikkt2ouk932.js` | [1782071,1782155) | `function eO(){let e=document.getElementById("booking-aside-container");if(!e)return;` |
| E08 | `scripts/0aikkt2ouk932.js` | [1741665,1741728) | `k=(0,t.jsxs)(B,{id:"booking-aside-container",children:[x,E,_]})` |
| E09 | `scripts/0aikkt2ouk932.js` | [1727756,1727843) | `return!!e&#124;&#124;"sold-out"===i&#124;&#124;"cancelled"===i&#124;&#124;!!t&#124;&#124;!!r&&("date-selection"===i?!a&#124;&#124;n&#124;&#124;o:n)` |
| E10 | `scripts/0aikkt2ouk932.js` | [1728296,1728363) | `if(!(await (0,k.ensureAuthenticated)(async()=>Y())).success)return;` |
| E11 | `scripts/0aikkt2ouk932.js` | [281223,281314) | `!K)return void(0,C.redirectToLogin)();if(!X)return void(0,C.triggerIdentityVerification)();` |
| E12 | `scripts/0aikkt2ouk932.js` | [288039,288100) | `"data-testid":"modal-booking-button",disabled:!u&#124;&#124;y,onClick:W` |
| E13 | `scripts/0aikkt2ouk932.js` | [141683,141734) | `BOOKING_DATE_TIME_PICKER:"booking_date_time_picker"` |
| E14 | `scripts/0aikkt2ouk932.js` | [274186,274249) | `1===i.length&&i[0]&&(r=i[0].playSeq,a=i[0].playDate),T(r??null)` |
| E15 | `scripts/0aikkt2ouk932.js` | [276096,276154) | `onChange:ey,value:eu,locale:"ko-KR",calendarType:"gregory"` |
| E16 | `scripts/0aikkt2ouk932.js` | [235918,235957) | `"abbr",{"aria-label":o(l,a),children:r}` |
| E17 | `scripts/0aikkt2ouk932.js` | [275815,275848) | `f=e=>{let{date:t}=e;return!eg(t)}` |
| E18 | `scripts/0aikkt2ouk932.js` | [265951,266004) | `"button",{"data-is-selected":r===s,onClick:()=>{c(r)}` |
| E19 | `scripts/0aikkt2ouk932.js` | [266336,266355) | `"time",{dateTime:a,` |
| E20 | `scripts/0aikkt2ouk932.js` | [142123,142244) | `let o=t.DateTime.now(),n=t.DateTime.fromFormat(e,i),l=t.DateTime.fromFormat(r,i);return n.isValid&&l.isValid?n<=o&&o<=l:a` |
| E21 | `scripts/134b7-pzle6r0.js` | [37924,37957) | `Settings.defaultZone="Asia/Seoul"` |
| E22 | `scripts/0aikkt2ouk932.js` | [212585,212627) | `setInterval(()=>{d(s.DateTime.now())},1e3)` |
| E23 | `scripts/0aikkt2ouk932.js` | [215441,215518) | `startDate:e.saleOpenTime,endDate:e.saleCloseTime,format:"yyyy-MM-dd HH:mm:ss"` |
| E24 | `scripts/0aikkt2ouk932.js` | [1736981,1737071) | `if((e.goodsStatus===l.GoodsStatus.Display&#124;&#124;u(e,t))&&t.length>0){r="open-scheduled-presale"` |
| E25 | `scripts/0aikkt2ouk932.js` | [149739,149836) | `a.searchParams.append("service","yanolja"),a.searchParams.append("redirect",window.location.href)` |
| E26 | `scripts/0aikkt2ouk932.js` | [150108,150253) | `return t?.isLoggedIn?t.isIdentityValid?{success:!0,authData:t}:(y(),{success:!1,reason:"not_verified"}):(b(),{success:!1,reason:"not_logged_in"})` |
| E27 | `scripts/0aikkt2ouk932.js` | [146209,146311) | `window.open("","authentication-window","width=480,height=823,top=100,left=100,menubar=no,location=no")` |
| E28 | `scripts/0aikkt2ouk932.js` | [217962,218011) | `new URL("/sso/v1/bridge/token",I.SSO_PAGE_ORIGIN)` |
| E29 | `scripts/0aikkt2ouk932.js` | [218234,218278) | `new URL("/gates/partner",I.GATE_PAGE_ORIGIN)` |
| E30 | `scripts/0aikkt2ouk932.js` | [218911,218992) | `(0,A.openUrlInNativeApp)({url:o.toString()})&#124;&#124;(window.location.href=o.toString())` |
| E31 | `scripts/17erxnd~wt7ql.js` | [645934,646088) | `s6=s1.string().parse("https://accounts.yanolja.com"),s3=s1.string().parse("https://tickets.interpark.com"),s5=s1.string().parse("https://sso.yanolja.com")` |
| E32 | `scripts/17erxnd~wt7ql.js` | [650121,650195) | `queryKey:["auth"],queryFn:()=>b({includeTier:!0}),staleTime:3e5,gcTime:6e5` |
| E33 | `scripts/0aikkt2ouk932.js` | [277659,277691) | `let i="bookingInitialSelection";` |
| E34 | `scripts/0aikkt2ouk932.js` | [288639,288691) | `children:"예매할 수 있는 날짜가 없어요."` |
| E35 | `scripts/0aikkt2ouk932.js` | [217271,217355) | `children:"일시적으로 에러가 발생했습니다. 다시 시도해 주세요."` |
| E36 | `scripts/0aikkt2ouk932.js` | [267735,267770) | `let{seatGradeName:r,remainCnt:i}=e;` |
