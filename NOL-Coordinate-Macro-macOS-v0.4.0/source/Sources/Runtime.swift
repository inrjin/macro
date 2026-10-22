import AppKit
import ScreenCaptureKit
import CoreMedia
import Carbon

func monoMS()->Double {ProcessInfo.processInfo.systemUptime*1000}
struct BrowserWindow {var id:CGWindowID,pid:pid_t,rect:CGRect}
func browserWindow()->BrowserWindow? {
    guard let front=NSWorkspace.shared.frontmostApplication,["com.google.Chrome","com.microsoft.edgemac","com.apple.Safari"].contains(front.bundleIdentifier ?? ""),
          let list=CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]],
          let w=list.first(where:{($0[kCGWindowOwnerPID as String] as? Int)==Int(front.processIdentifier) && ($0[kCGWindowLayer as String] as? Int)==0}),
          let bounds=w[kCGWindowBounds as String] as? [String:Any],let rect=CGRect(dictionaryRepresentation:bounds as CFDictionary),rect.width>100,rect.height>100,
          let id=w[kCGWindowNumber as String] as? UInt32 else{return nil}
    return BrowserWindow(id:id,pid:front.processIdentifier,rect:rect)
}
final class NoRedirect:NSObject,URLSessionTaskDelegate {
    func urlSession(_ session:URLSession,task:URLSessionTask,willPerformHTTPRedirection response:HTTPURLResponse,newRequest request:URLRequest,completionHandler:@escaping(URLRequest?)->Void){completionHandler(nil)}
}
func measureClock(_ c:Settings) async throws -> ClockSample {
    guard let u=URL(string:c.clockURL),u.scheme=="https",u.user==nil,u.password==nil else{throw fail("HTTPS 시각 API 주소를 입력하세요.")}
    let config=URLSessionConfiguration.ephemeral;config.timeoutIntervalForResource=6;config.httpCookieStorage=nil;config.httpShouldSetCookies=false;config.urlCache=nil;config.requestCachePolicy = .reloadIgnoringLocalCacheData
    let session=URLSession(configuration:config,delegate:NoRedirect(),delegateQueue:nil);defer{session.invalidateAndCancel()}
    var samples:[ClockSample]=[]
    for _ in 0..<3 {
        var req=URLRequest(url:u);req.httpMethod=c.clockMode=="date" ? "HEAD":"GET";req.timeoutInterval=5;req.setValue("no-cache",forHTTPHeaderField:"Cache-Control")
        let start=monoMS();let(data,response)=try await session.data(for:req);let end=monoMS()
        guard let h=response as? HTTPURLResponse,(200..<300).contains(h.statusCode),data.count<=65536,(Double(h.value(forHTTPHeaderField:"Age") ?? "0") ?? 1)<=0,end-start<=3000 else{throw fail("시각 API의 캐시·크기·지연·응답 상태를 확인하세요.")}
        guard let(server,precision)=ClockParser.value(data:data,dateHeader:h.value(forHTTPHeaderField:"Date"),mode:c.clockMode,path:c.clockPath) else{throw fail("시각 API 형식 또는 JSON 경로가 맞지 않습니다.")}
        samples.append(ClockSample(server:server,mono:(start+end)/2,rtt:end-start,uncertainty:(end-start+precision)/2))
    }
    return samples.min(by:{$0.rtt<$1.rtt})!
}
final class CaptureSink:NSObject,SCStreamOutput,SCStreamDelegate {
    var receive:((SCStream,CVPixelBuffer?,Double,Bool)->Void)?,failed:((SCStream,String)->Void)?
    func stream(_ stream:SCStream,didOutputSampleBuffer sampleBuffer:CMSampleBuffer,of outputType:SCStreamOutputType){
        guard outputType == .screen else{return}
        guard CMSampleBufferIsValid(sampleBuffer),
              let attachments=CMSampleBufferGetSampleAttachmentsArray(sampleBuffer,createIfNecessary:false) as? [[SCStreamFrameInfo:Any]],
              let raw=attachments.first?[.status] as? Int,let state=SCFrameStatus(rawValue:raw) else{return}
        let now=monoMS()
        if state == .idle {DispatchQueue.main.async{self.receive?(stream,nil,now,true)};return}
        guard state == .complete || state == .started else{
            DispatchQueue.main.async{self.failed?(stream,"화면이 꺼졌거나 캡처가 일시 중지되었습니다.")};return
        }
        guard let frame=CMSampleBufferGetImageBuffer(sampleBuffer),CVPixelBufferGetPixelFormatType(frame)==kCVPixelFormatType_32BGRA else{return}
        DispatchQueue.main.async{self.receive?(stream,frame,now,false)}
    }
    func stream(_ stream:SCStream,didStopWithError error:Error){DispatchQueue.main.async{self.failed?(stream,error.localizedDescription)}}
}
@MainActor final class CoordinateEngine:ObservableObject {
    @Published var settings=Settings()
    @Published var status="F7로 브라우저 연결 → F8로 좌표/색 채집 → F1 일반 / F2 취소표"
    @Published var captureKey="SeatTL"
    @Published var verified=false
    @Published var running=false
    @Published var boundText="연결 안 됨"
    @Published var clockText="측정 안 됨"
    @Published var phase="STOPPED"
    @Published var syncing=false
    var bound:BrowserWindow?,stream:SCStream?,sink=CaptureSink(),frame:CVPixelBuffer?,frameTime=0.0,frameID=0,lastScan = -1,lastInputFrame = -1,lastInputTime=0.0,frameIdle=false
    var clock:ClockSample?,clockIdentity="",timer:Timer?,hotkeys:[EventHotKeyRef]=[],handler:EventHandlerRef?
    var cfg=Settings(),cancel=false,due=0.0,deadline=0.0,searchEnd=0.0,resultEnd=0.0,zone=1,failures=0,step=0,readyMiss=false,readyCount=0
    var selected:[CGPoint]=[],rejected:[Int:[CGPoint]]=[:],candidate:CGPoint?,confirmPixel:CGPoint?,priceSeen=false,readyAt=0.0,freshUntil=0.0
    var testClick:((CGPoint)->Void)?
    let file=FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/NOLCoordinateMacro/settings.json")
    init(testing:Bool=false){
        if !testing,let data=try? Data(contentsOf:file),let c=try? JSONDecoder().decode(Settings.self,from:data){settings=c}
        sink.receive={[weak self] stream,f,t,idle in guard let self=self,self.stream === stream else{return};if let f=f{self.frame=f;self.frameID+=1};self.frameTime=t;self.frameIdle=idle}
        sink.failed={[weak self] stream,e in guard let self=self,self.stream === stream else{return};self.frame=nil;self.frameIdle=false;self.stop("화면 캡처 종료: "+e)}
        if !testing {registerKeys();timer=Timer.scheduledTimer(withTimeInterval:0.002,repeats:true){[weak self] _ in MainActor.assumeIsolated{self?.tick()}};RunLoop.main.add(timer!,forMode:.common)}
    }
    func save(){do{try FileManager.default.createDirectory(at:file.deletingLastPathComponent(),withIntermediateDirectories:true);try JSONEncoder().encode(settings).write(to:file,options:.atomic);status="설정 저장 완료"}catch{status=error.localizedDescription}}
    func registerKeys(){
        var event=EventTypeSpec(eventClass:OSType(kEventClassKeyboard),eventKind:UInt32(kEventHotKeyPressed))
        InstallEventHandler(GetApplicationEventTarget(),{_,event,info in
            guard let event=event,let info=info else{return noErr};var id=EventHotKeyID();GetEventParameter(event,EventParamName(kEventParamDirectObject),EventParamType(typeEventHotKeyID),nil,MemoryLayout<EventHotKeyID>.size,nil,&id)
            let engine=Unmanaged<CoordinateEngine>.fromOpaque(info).takeUnretainedValue();let key=id.id
            Task{@MainActor in engine.key(key)};return noErr
        },1,&event,Unmanaged.passUnretained(self).toOpaque(),&handler)
        for(key,code) in [(1,kVK_F1),(2,kVK_F2),(3,kVK_F3),(4,kVK_F4),(6,kVK_F6),(7,kVK_F7),(8,kVK_F8)] {
            var ref:EventHotKeyRef?;let id=EventHotKeyID(signature:0x4E4F4C43,id:UInt32(key))
            if RegisterEventHotKey(UInt32(code),0,id,GetApplicationEventTarget(),0,&ref)==noErr,let ref=ref{hotkeys.append(ref)}
        }
    }
    func key(_ n:UInt32){switch n{case 1:start(cancel:false);case 2:start(cancel:true);case 3:stop("F3 긴급 중지");case 4:start(cancel:false,scheduled:true);case 6:resume();case 7:Task{await bind()};case 8:capture();default:break}}
    func bind() async {
        let humanRebind=running && phase=="HUMAN"
        guard !running || humanRebind else{return}
        guard CGPreflightScreenCaptureAccess() else{status="화면 기록 권한이 필요합니다. 권한 버튼을 눌러 허용한 뒤 앱을 다시 여세요.";return}
        guard let b=browserWindow() else{status="대상 Chrome/Edge/Safari를 전면에 두고 F7을 누르세요.";return}
        do{
            let content=try await SCShareableContent.excludingDesktopWindows(true,onScreenWindowsOnly:true)
            guard let display=content.displays.first(where:{$0.frame.contains(b.rect)}) else{throw fail("브라우저 창을 한 모니터 안에 넣어 주세요.")}
            if let previous=stream {stream=nil;try await previous.stopCapture()}
            let c=SCStreamConfiguration();c.width=Int(b.rect.width);c.height=Int(b.rect.height);c.sourceRect=b.rect.offsetBy(dx:-display.frame.minX,dy:-display.frame.minY);c.minimumFrameInterval=CMTime(value:1,timescale:120);c.pixelFormat=kCVPixelFormatType_32BGRA;c.showsCursor=false;c.queueDepth=3
            let s=SCStream(filter:SCContentFilter(display:display,excludingWindows:[]),configuration:c,delegate:sink)
            try s.addStreamOutput(sink,type:.screen,sampleHandlerQueue:DispatchQueue(label:"nol.pixel.capture",qos:.userInteractive))
            frame=nil;frameTime=0;frameIdle=false;stream=s;bound=b;try await s.startCapture();verified=false;boundText="\(Int(b.rect.width)) × \(Int(b.rect.height)) · 창 \(b.id)"
            if humanRebind {try cfg.validate(size:b.rect.size,cancel:false);cfg.autoReady=false;status="예매 창 재연결 완료. 캡챠 입력 후 F6으로 재개하세요."}
            else {status="연결 완료. 위치·크기·배율·스크롤을 유지하고 좌표를 지정하세요."}
        }catch{stop(error.localizedDescription);bound=nil}
    }
    func guarded(_ point:CGPoint?=nil)->Bool {
        if testClick != nil{return true}
        guard let b=bound,let current=browserWindow(),current.id==b.id,current.pid==b.pid,current.rect==b.rect else{stop("브라우저 포커스·창 위치·크기가 바뀌어 중지했습니다.");return false}
        if let p=point {
            guard p.x>=0,p.y>=0,p.x<b.rect.width,p.y<b.rect.height else{stop("클릭 위치가 창 밖입니다.");return false}
            let screen=CGPoint(x:b.rect.minX+p.x,y:b.rect.minY+p.y)
            let windows=(CGWindowListCopyWindowInfo([.optionOnScreenOnly,.excludeDesktopElements],kCGNullWindowID) as? [[String:Any]]) ?? []
            for w in windows {guard let bounds=w[kCGWindowBounds as String] as? [String:Any],let r=CGRect(dictionaryRepresentation:bounds as CFDictionary),r.contains(screen),(w[kCGWindowAlpha as String] as? Double ?? 1)>0 else{continue};if (w[kCGWindowNumber as String] as? UInt32) != b.id {stop("클릭 위치가 다른 창에 가려져 있습니다.");return false};break}
        }
        return true
    }
    func click(_ p:CGPoint)->Bool {
        guard running,guarded(p) else{return false}
        lastInputFrame=frameID;lastInputTime=monoMS()
        if let testClick=testClick{testClick(p);return true}
        guard AXIsProcessTrusted(),let b=bound else{stop("손쉬운 사용 권한이 필요합니다.");return false}
        let s=CGPoint(x:b.rect.minX+p.x,y:b.rect.minY+p.y)
        CGEvent(mouseEventSource:nil,mouseType:.leftMouseDown,mouseCursorPosition:s,mouseButton:.left)?.post(tap:.cghidEventTap)
        CGEvent(mouseEventSource:nil,mouseType:.leftMouseUp,mouseCursorPosition:s,mouseButton:.left)?.post(tap:.cghidEventTap)
        return true
    }
    func capture(){
        guard !running,guarded(),let b=bound,let mouse=CGEvent(source:nil)?.location,let f=frame,(frameIdle || monoMS()-frameTime<1000) else{status="F7 연결 후 브라우저 위에 마우스를 두고 F8을 누르세요.";return}
        let p=CGPoint(x:mouse.x-b.rect.minX,y:mouse.y-b.rect.minY)
        CVPixelBufferLockBaseAddress(f,.readOnly);defer{CVPixelBufferUnlockBaseAddress(f,.readOnly)}
        let pixels=pixelView(f);guard let color=pixels.color(p) else{return}
        let s=Spot(x:String(Int(p.x)),y:String(Int(p.y)),color:String(format:"#%06X",color))
        if captureKey.hasPrefix("step:") {
            let parts=captureKey.split(separator:":")
            guard parts.count==3,let id=UUID(uuidString:String(parts[1])),let i=settings.steps.firstIndex(where:{$0.id==id}) else{return}
            if parts[2]=="click"{settings.steps[i].target=s}else{settings.steps[i].probe=s}
        }else if captureKey=="Step" || captureKey=="StepWait" {
            guard !settings.steps.isEmpty else{status="후속 단계를 먼저 추가하세요.";return}
            if captureKey=="Step" {settings.steps[settings.steps.count-1].target=s}else{settings.steps[settings.steps.count-1].probe=s}
        }else{settings.spots[captureKey]=s}
        verified=false;status="\(captureKey) 저장: (\(s.x), \(s.y)) \(s.color)"
    }
    func pixelView(_ f:CVPixelBuffer)->Pixels{Pixels(bytes:CVPixelBufferGetBaseAddress(f)!.assumingMemoryBound(to:UInt8.self),width:CVPixelBufferGetWidth(f),height:CVPixelBufferGetHeight(f),stride:CVPixelBufferGetBytesPerRow(f))}
    func sync(){guard !running,!syncing else{return};syncing=true;let c=settings;clock=nil;clockText="3회 측정 중…";Task{defer{syncing=false};do{let v=try await measureClock(c);guard c.clockURL==settings.clockURL,c.clockMode==settings.clockMode,c.clockPath==settings.clockPath else{throw fail("측정 중 설정이 바뀌었습니다.")};clock=v;clockIdentity=c.clockURL+"|"+c.clockMode+"|"+c.clockPath;clockText=String(format:"RTT %.1f ms · 추정 오차 ±%.1f ms",v.rtt,v.uncertainty)}catch{clockText=error.localizedDescription}}}
    func start(cancel:Bool,scheduled:Bool=false){
        guard !running else{return}
        do{
            guard verified,let b=bound,guarded() else{throw fail("F7 연결과 좌표 확인 체크가 필요합니다.")}
            guard AXIsProcessTrusted() || testClick != nil else{throw fail("손쉬운 사용 권한을 허용하세요.")}
            try settings.validate(size:b.rect.size,cancel:cancel,scheduled:scheduled)
            cfg=settings;self.cancel=cancel;selected=[];rejected=[:];candidate=nil;zone=1;failures=0;step=0;readyMiss=false;readyCount=0;lastScan = -1;lastInputFrame = -1;priceSeen=false;readyAt=0;freshUntil=0
            let now=monoMS();due=now;searchEnd=now+(cancel ? cfg.cancelTimeout:cfg.searchTimeout);deadline=searchEnd+120000+(cfg.follow ? cfg.steps.filter{$0.enabled}.reduce(0){$0+$1.timeout}:0)
            if scheduled {
                guard let sample=clock,now-sample.mono<600000,clockIdentity==cfg.clockURL+"|"+cfg.clockMode+"|"+cfg.clockPath,let target=ClockParser.target(cfg.saleTime) else{throw fail("시각 API를 측정하고 예약 한국시간을 밀리초까지 입력하세요.")}
                due=sample.deadline(target);guard due>now,due-now<=43_200_000 else{throw fail("예약 시각은 12시간 이내 미래여야 합니다.")};deadline=due+900000+cfg.searchTimeout+(cfg.follow ? cfg.steps.filter{$0.enabled}.reduce(0){$0+$1.timeout}:0);phase="OPENING"
            }else{phase=cancel ? (cfg.twoZones ? "ZONE":"SEARCH"):(cfg.useRegion ? "REGION":"SEARCH")}
            running=true;status=scheduled ? "예약 대기 · F3 중지":"\(cancel ? "취소표":"일반") 탐색 · F3 중지"
        }catch{status=error.localizedDescription}
    }
    func resume(){guard running,phase=="HUMAN",guarded() else{return};phase=cfg.useRegion ? "REGION":"SEARCH";due=monoMS();searchEnd=due+cfg.searchTimeout;status="캡챠 이후 구역/좌석 진행"}
    func stop(_ message:String){running=false;phase="STOPPED";candidate=nil;status=message}
    func tick(){
        guard running else{return}
        let now=monoMS()
        guard now<deadline else{stop("전체 실행 제한시간이 지났습니다.");return}
        if phase=="HUMAN",testClick==nil {
            guard let b=bound,let current=browserWindow(),current.id==b.id,current.rect==b.rect else{let message="캡챠 직접 입력 · 새 예매 창이면 그 창에서 F7 재연결 후 F6";if status != message{status=message};return}
        }else if testClick==nil,NSWorkspace.shared.frontmostApplication?.processIdentifier != bound?.pid {stop("브라우저가 전면이 아니어서 중지했습니다.");return}
        if phase=="OPENING",now>=due {if click(cfg.spot("Open").point!){phase="HUMAN";status="예매 버튼 클릭 완료 · 캡챠 입력 후 F6"};return}
        guard let f=frame,frameIdle || now-frameTime<1000 else {if phase != "OPENING" {stop("최신 화면을 받을 수 없습니다. F7로 다시 연결하세요.")};return}
        CVPixelBufferLockBaseAddress(f,.readOnly);defer{CVPixelBufferUnlockBaseAddress(f,.readOnly)}
        let pixels=pixelView(f)
        for _ in 0..<8 {let before=phase;process(pixels,now);if !running || phase==before || now<due{break}}
    }
    func process(_ p:Pixels,_ now:Double){
        guard running else{return}
        func probe(_ key:String)->Bool{p.matches(cfg.spot(key),cfg.tolerance)}
        if cfg.conflict,candidate != nil,["CONFIRM","COMPLETE","RESULT","SEARCH"].contains(phase),probe("ConflictA"),probe("ConflictB") {
            guard !selected.contains(where:{$0 != candidate}) else{stop("여러 좌석 선택 중 경합입니다. 어떤 좌석이 해제됐는지 직접 확인하세요.");return}
            failures+=1;guard failures<=cfg.retryLimit else{stop("경합 재시도 제한 도달");return}
            rejected[zone,default:[]].append(candidate!);selected.removeAll{$0==candidate};phase="DISMISS";resultEnd=now+cfg.resultTimeout;due=now
            _=click(cfg.spot("Dismiss").point!);return
        }
        guard now>=due else{return}
        switch phase {
        case "OPENING":break
        case "HUMAN":
            if cfg.autoReady,frameID>lastInputFrame {
                if !probe("Ready"){readyMiss=true;readyAt=0}
                else if readyMiss {if readyAt==0{readyAt=now}else if now-readyAt>=max(16,cfg.poll){phase=cfg.useRegion ? "REGION":"SEARCH";searchEnd=now+cfg.searchTimeout;due=now}}
            }
        case "REGION":if click(cfg.spot("Region").point!){phase="SEARCH";due=now+cfg.regionDelay;searchEnd=now+cfg.searchTimeout;lastScan = -1;freshUntil=now+cfg.resultTimeout}
        case "ZONE":if click(cfg.spot("Zone\(zone)").point!){phase="SEARCH";due=now+cfg.zoneDelay;lastScan = -1;freshUntil=now+cfg.resultTimeout}
        case "SEARCH":
            guard now<searchEnd else{stop("좌석 탐색 제한시간 만료");return}
            if freshUntil>0,frameID<=lastInputFrame,!(frameIdle && frameTime>lastInputTime) {
                if now>=freshUntil {
                    if cancel && cfg.twoZones {zone=zone==1 ? 2:1;phase="ZONE";due=now}
                    else{stop("구역 클릭 후 새 화면이 없습니다. 구역 좌표와 대기를 확인하세요.")}
                };return
            }
            freshUntil=0
            guard frameID != lastScan else{return};lastScan=frameID
            let id=cancel ? "Zone\(zone)":"Seat",rgb=cfg.spot(id+"Color").rgb!
            if let found=p.find(a:cfg.spot(id+"TL").point!,b:cfg.spot(id+"BR").point!,rgb:rgb,tolerance:cfg.tolerance,excluded:selected+(rejected[zone] ?? []),gap:cfg.gap) {
                candidate=found;confirmPixel=found;phase="CLICK";due=now+cfg.clickDelay
            }else if cancel && cfg.twoZones{zone=zone==1 ? 2:1;phase="ZONE";due=now+cfg.poll}
        case "CLICK":
            let id=cancel ? "Zone\(zone)":"Seat"
            guard let point=candidate,let color=p.color(point),Pixels.matches(color,cfg.spot(id+"Color").rgb!,cfg.tolerance) else{phase="SEARCH";lastScan = -1;return}
            if click(point){phase="CONFIRM";resultEnd=now+cfg.confirmTimeout;due=now+cfg.poll}
        case "CONFIRM":
            guard frameID>lastInputFrame else{if now>=resultEnd{stop("좌석 클릭 뒤 새 화면이 없습니다.")};return}
            let id=cancel ? "Zone\(zone)":"Seat"
            if let at=confirmPixel,let color=p.color(at),!Pixels.matches(color,cfg.spot(id+"Color").rgb!,cfg.tolerance) {
                selected.append(candidate!);phase=selected.count >= (cancel ? 1:cfg.seats) ? "COMPLETE":"SEARCH";lastScan = -1;due=now+cfg.poll
            }else if now>=resultEnd{stop("클릭 뒤 색 변화가 없습니다. 화면을 확인하세요.")}
        case "COMPLETE":
            let id=cancel ? "Zone\(zone)":"Seat"
            if selected.contains(where:{point in guard let c=p.color(point) else{return true};return Pixels.matches(c,cfg.spot(id+"Color").rgb!,cfg.tolerance)}){stop("선택한 좌석이 원래 색으로 돌아왔습니다.");return}
            if click(cfg.spot(cancel ? "CancelDone":"Done").point!){phase=cfg.nextReady ? "RESULT":"NEXT";resultEnd=now+cfg.resultTimeout;due=now}
        case "DISMISS":
            if frameID>lastInputFrame,!(probe("ConflictA")&&probe("ConflictB")){phase="SEARCH";lastScan = -1;due=now+cfg.poll}
            else if now>=resultEnd{stop("경합 안내가 닫히지 않았습니다.")}
        case "RESULT":
            if frameID>lastInputFrame,probe("Success"){phase="NEXT";due=now;priceSeen=true}
            else if now>=resultEnd{stop("가격 화면 준비 표식이 없습니다. 직접 확인하세요.")}
        case "NEXT":
            let list=cfg.follow ? cfg.steps.filter{$0.enabled}:[]
            guard step<list.count else{stop("지정 클릭 완료. 좌석 확보와 예매 정보 화면은 직접 확인하세요.");return}
            due=now+list[step].delay;resultEnd=now+list[step].timeout;phase="STEP"
        case "STEP":
            let s=cfg.steps.filter{$0.enabled}[step]
            if now>=resultEnd{stop("후속 단계 제한시간 만료: "+s.name);return}
            if !s.wait || (frameID>lastInputFrame && p.matches(s.probe,cfg.tolerance)){if click(s.target.point!){step+=1;phase="NEXT";due=now}}
        default:stop("알 수 없는 단계")
        }
    }
}
