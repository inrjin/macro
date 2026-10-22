import SwiftUI
import AppKit

final class AppDelegate:NSObject,NSApplicationDelegate {
    func applicationShouldTerminateAfterLastWindowClosed(_ sender:NSApplication)->Bool {true}
}
@main struct CoordinateMacroApp:App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    @StateObject var model:CoordinateEngine
    init(){
        if CommandLine.arguments.contains("--self-test") {
            do{let checks=try coreTests()+runtimeTests();print("PASS: \(checks.count) test groups: \(checks.joined(separator:", "))");exit(0)}catch{print("FAIL: \(error.localizedDescription)");exit(1)}
        }
        _model=StateObject(wrappedValue:CoordinateEngine())
    }
    var body:some Scene{WindowGroup("NOL 좌표 매크로 0.4.0"){MacroView(m:model)}.defaultSize(width:960,height:740).windowResizability(.contentMinSize)}
}
struct MacroView:View {
    @ObservedObject var m:CoordinateEngine
    func spot(_ key:String,_ field:WritableKeyPath<Spot,String>)->Binding<String>{Binding(get:{m.settings.spot(key)[keyPath:field]},set:{var s=m.settings.spot(key);s[keyPath:field]=$0;m.settings.spots[key]=s;m.verified=false})}
    func point(_ key:String,_ title:String,color:Bool=false)->some View {
        HStack{Text(title).frame(width:180,alignment:.leading);TextField("X",text:spot(key,\.x)).frame(width:68);TextField("Y",text:spot(key,\.y)).frame(width:68)
            if color{TextField("#RRGGBB",text:spot(key,\.color)).frame(width:100)}
            Button("F8 채집 대상으로 지정"){m.captureKey=key}.font(.caption)
        }
    }
    func number(_ title:String,_ key:WritableKeyPath<Settings,Double>)->some View{HStack{Text(title);Spacer();TextField("ms",value:Binding(get:{m.settings[keyPath:key]},set:{m.settings[keyPath:key]=$0}),format:.number).frame(width:120);Text("ms").foregroundStyle(.secondary)}}
    var body:some View {
        VStack(alignment:.leading,spacing:10){
            HStack{Text("NOL 좌표 매크로").font(.title.bold());Spacer();Text(m.running ? m.phase:"준비").font(.headline).foregroundStyle(m.running ? .green:.secondary)}
            Text("F7 창 연결   F8 좌표·색 채집   F1 일반   F2 취소표   F4 예약   F6 캡챠 뒤 재개   F3 긴급 중지").font(.callout)
            HStack{Text(m.boundText);Spacer();Button("중지 (F3)"){m.stop("사용자 중지")}.tint(.red);Button("설정 저장"){m.save()}.disabled(m.running)}
            Toggle("현재 창 크기·위치·배율·스크롤에서 좌표와 색상을 확인했습니다",isOn:$m.verified).disabled(m.running)
            TabView{
                opening.tabItem{Label("시각·진입",systemImage:"clock")}
                general.tabItem{Label("일반 티켓팅",systemImage:"cursorarrow.click")}
                cancel.tabItem{Label("취소표",systemImage:"arrow.triangle.2.circlepath")}
                outcomes.tabItem{Label("경합·전환",systemImage:"arrow.turn.down.right")}
                following.tabItem{Label("후속 클릭",systemImage:"list.number")}
                help.tabItem{Label("권한·사용법",systemImage:"questionmark.circle")}
            }.disabled(m.running)
            Text("F8 채집 항목: \(m.captureKey) · 좌표는 연결한 브라우저 창 왼쪽 위 기준입니다.").font(.caption)
            Text(m.status).font(.callout).textSelection(.enabled).frame(maxWidth:.infinity,alignment:.leading).padding(10).background(.quaternary).cornerRadius(8)
        }.padding(20).frame(minWidth:900,idealWidth:960,maxWidth:.infinity,minHeight:620,idealHeight:740,maxHeight:.infinity)
    }
    var opening:some View{Form{
        Section("서버 시각 · 등록한 API만 3회 조회"){
            TextField("HTTPS API 주소",text:$m.settings.clockURL)
            Picker("응답 형식",selection:$m.settings.clockMode){Text("Unix 밀리초").tag("milliseconds");Text("Unix 초").tag("seconds");Text("ISO 시각").tag("iso");Text("HTTP Date 헤더").tag("date")}
            TextField("JSON 필드 경로 · 예: data.timestamp · 본문이 숫자면 비움",text:$m.settings.clockPath)
            TextField("예약 한국시간 · YYYY-MM-DD HH:mm:ss.SSS",text:$m.settings.saleTime)
            HStack{Button("시각 측정"){m.sync()}.disabled(m.syncing);Text(m.clockText).font(.caption)}
            point("Open","예약 예매 버튼")
            Text("측정 후 브라우저에서 F4를 누르면 보정된 시각에 이 좌표를 한 번 클릭합니다. API가 예매 서버와 같은 시각을 제공하는지는 사용자가 확인해야 합니다.").font(.caption)
        }
        Section("캡챠 직접 입력 이후"){
            Toggle("구역 준비 픽셀 변화로 자동 재개 · 끄면 F6",isOn:$m.settings.autoReady)
            point("Ready","준비 픽셀",color:true)
            Text("자동 재개는 예약 클릭 뒤 지정 색과 불일치한 화면을 먼저 관찰하고, 이후 일치 상태가 2회 유지되는지 확인합니다. 캡챠가 떠 있을 때도 같은 색인 위치는 사용하지 마세요.").font(.caption)
        }
    }.formStyle(.grouped)}
    var general:some View{Form{
        Section("미리 정한 구역 1곳"){
            Toggle("F1/F6 시작 시 구역 좌표 클릭",isOn:$m.settings.useRegion)
            point("Region","구역 이동")
            number("구역 클릭 후 최소 대기",\.regionDelay)
        }
        Section("좌석 검색 영역"){
            point("SeatTL","검색 범위 ↖");point("SeatBR","검색 범위 ↘");point("SeatColor","좌석 색",color:true);point("Done","선택 완료")
            Stepper("좌석 수 \(m.settings.seats) · 연석 보장 없음",value:$m.settings.seats,in:1...4)
            number("일반 좌석 탐색 제한",\.searchTimeout)
        }
        Section("속도·색상"){
            number("좌석 클릭 전 대기",\.clickDelay);number("반응 확인 간격",\.poll)
            number("클릭 뒤 색 변화 제한",\.confirmTimeout)
            Stepper("색 허용 오차 \(m.settings.tolerance)",value:$m.settings.tolerance,in:0...64)
            Stepper("같은 좌석 제외 반경 \(m.settings.gap) px",value:$m.settings.gap,in:1...100)
        }
    }.formStyle(.grouped)}
    var cancel:some View{Form{
        Section("F2 취소표 탐색 · 1석"){
            Toggle("지정한 두 구역 번갈아 이동 · 끄면 현재 1구역 검색",isOn:$m.settings.twoZones)
            point("Zone1","1구역 이동");point("Zone1TL","1구역 범위 ↖");point("Zone1BR","1구역 범위 ↘");point("Zone1Color","1구역 좌석 색",color:true)
            point("Zone2","2구역 이동");point("Zone2TL","2구역 범위 ↖");point("Zone2BR","2구역 범위 ↘");point("Zone2Color","2구역 좌석 색",color:true)
            point("CancelDone","취소표 선택 완료")
            number("구역 이동 후 대기",\.zoneDelay);number("취소표 탐색 제한 · 기본 1시간",\.cancelTimeout)
            Text("현재 화면에 새 좌석이 표시되면 탐색합니다. 사이트가 재조회하지 않는 화면에서는 두 구역 이동을 사용하세요. 자동 새로고침이나 좌석 API 조회는 하지 않습니다.").font(.caption)
        }
    }.formStyle(.grouped)}
    var outcomes:some View{Form{
        Section("경합 안내 확인 후 다른 좌석"){
            Toggle("두 픽셀의 지정 색이 모두 일치하면 경합으로 처리",isOn:$m.settings.conflict)
            point("ConflictA","경합 표식 A",color:true);point("ConflictB","경합 표식 B",color:true);point("Dismiss","경합 안내 닫기")
            Stepper("최대 재시도 \(m.settings.retryLimit)",value:$m.settings.retryLimit,in:1...100)
            Text("경합 안내와 다른 안내를 구별할 수 있는 두 위치를 채집하세요. 글자나 서버 응답을 해석하지 않으며, 실패한 좌석은 이번 실행에서 제외합니다.").font(.caption)
        }
        Section("선택 완료 → 다음 화면"){
            Toggle("가격 화면 준비 색을 기다림 · 후속 클릭에 필수",isOn:$m.settings.nextReady)
            point("Success","가격 화면 준비",color:true);number("다음 화면 대기 제한",\.resultTimeout)
            Text("좌석의 색 변화만으로 확보 성공을 확정하지 않습니다. 가격 화면의 고유 표식을 지정하면 경합 안내 또는 전환을 기다립니다.").font(.caption)
        }
    }.formStyle(.grouped)}
    var following:some View{Form{
        Toggle("활성 단계만 순서대로 각 1회 실행",isOn:$m.settings.follow)
        Text("예: 일반 가격 + 버튼 → 1매 예매하기. 각 화면의 준비 픽셀을 지정하고 예매 정보 입력 화면에서 끝내세요.").font(.caption)
        ForEach($m.settings.steps){$s in
            Section(s.name){
                Toggle("사용",isOn:$s.enabled);TextField("이름",text:$s.name)
                HStack{Text("클릭 X/Y");TextField("X",text:$s.target.x);TextField("Y",text:$s.target.y)}
                HStack{Text("최소 대기 ms");TextField("ms",value:$s.delay,format:.number);Text("제한 ms");TextField("ms",value:$s.timeout,format:.number)}
                Toggle("화면 픽셀 색 대기",isOn:$s.wait)
                HStack{TextField("대기 X",text:$s.probe.x);TextField("대기 Y",text:$s.probe.y);TextField("#RRGGBB",text:$s.probe.color)}
                HStack{
                    Button("클릭 좌표 F8"){m.captureKey="step:\(s.id.uuidString):click"}
                    Button("대기 픽셀 F8"){m.captureKey="step:\(s.id.uuidString):wait"}
                    Button("↑"){move(s.id,-1)};Button("↓"){move(s.id,1)}
                    Button("삭제"){m.settings.steps.removeAll{$0.id==s.id}}
                }
            }
        }
        Button("후속 단계 추가"){if m.settings.steps.count<20{m.settings.steps.append(ClickStep())}}
    }.formStyle(.grouped)}
    func move(_ id:UUID,_ offset:Int){guard let i=m.settings.steps.firstIndex(where:{$0.id==id}),m.settings.steps.indices.contains(i+offset)else{return};m.settings.steps.swapAt(i,i+offset)}
    var help:some View{Form{
        Section("권한"){
            Button("화면 기록 권한 요청"){_ = CGRequestScreenCaptureAccess()}
            Button("손쉬운 사용 설정 열기"){NSWorkspace.shared.open(URL(string:"x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)}
            Text("앱에 화면 기록·손쉬운 사용을 허용한 뒤 다시 실행하세요. 녹화 파일은 저장하지 않습니다. 확장 프로그램이나 사이트 로그인 연동은 필요하지 않습니다.").font(.caption)
        }
        Section("사용 순서"){
            Text("1. 브라우저를 전면에 두고 F7로 연결합니다.\n2. 각 설정에서 F8 채집 대상을 고른 뒤, 브라우저의 해당 위치 위에서 F8을 누릅니다. 클릭은 보내지 않습니다.\n3. 좌표 확인을 체크합니다. F1 일반 / F2 취소표 / 시각 측정 후 F4 예약을 선택합니다.\n4. 캡챠 직접 입력 후 F6으로 재개합니다. 새 예매 창이면 먼저 F7로 재연결합니다. F3은 언제든 완전 중지합니다.\n5. 창 위치·크기·배율·탭·스크롤을 바꾸지 마세요. 배치가 달라지면 다시 지정합니다.")
            Text("Mac 기능키가 밝기/음량으로 동작하면 fn과 함께 누르거나 시스템 설정에서 표준 기능키를 켜세요. 검색 속도는 영역 크기, 디스플레이 갱신율과 시스템 부하에 영향을 받습니다.").font(.caption)
        }
    }.formStyle(.grouped)}
}
