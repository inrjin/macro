import Foundation

struct MacroError: LocalizedError { let message: String; var errorDescription: String? { message } }
func fail(_ message: String) -> MacroError { MacroError(message: message) }
struct Spot: Codable, Equatable {
    var x = "", y = "", color = ""
    var point: CGPoint? { guard let x=Double(x), let y=Double(y), x.isFinite, y.isFinite, x>=0, y>=0 else {return nil}; return CGPoint(x:x,y:y) }
    var rgb: UInt32? { let s=color.replacingOccurrences(of:"#",with:""); guard s.count==6 else{return nil}; return UInt32(s,radix:16) }
}
struct ClickStep: Codable, Identifiable {
    var id=UUID(), enabled=true, name="추가 클릭", target=Spot(), delay=0.0, wait=false, probe=Spot(), timeout=5000.0
}
struct Settings: Codable {
    var spots: [String:Spot] = [:]
    var useRegion=true, twoZones=false, autoReady=false, conflict=false, nextReady=false, follow=false
    var seats=1, tolerance=3, gap=12, retryLimit=30
    var clickDelay=0.0, regionDelay=150.0, zoneDelay=400.0, poll=8.0, resultTimeout=5000.0, searchTimeout=30000.0, cancelTimeout=3600000.0, confirmTimeout=1500.0
    var clockURL="", clockPath="", clockMode="milliseconds", saleTime=""
    var steps:[ClickStep]=[]
    func spot(_ key:String)->Spot { spots[key] ?? Spot() }
    func validate(size:CGSize,cancel:Bool,scheduled:Bool=false) throws {
        func point(_ key:String) throws { guard let p=spot(key).point,p.x<size.width,p.y<size.height else {throw fail("\(key) 좌표를 F8로 지정하세요.")} }
        func color(_ key:String) throws {try point(key); guard spot(key).rgb != nil else {throw fail("\(key) 색상을 지정하세요.")} }
        func rect(_ key:String) throws {try point(key+"TL");try point(key+"BR");let a=spot(key+"TL").point!,b=spot(key+"BR").point!; guard b.x>a.x,b.y>a.y,(b.x-a.x)*(b.y-a.y)<=4_000_000 else {throw fail("\(key) 검색 범위 ↖/↘를 확인하세요.")};guard spot(key+"Color").rgb != nil else {throw fail("\(key) 좌석 색을 지정하세요.")} }
        guard (1...4).contains(seats),(0...64).contains(tolerance),(1...100).contains(gap),(1...100).contains(retryLimit),[clickDelay,regionDelay,zoneDelay,poll,resultTimeout,searchTimeout,cancelTimeout,confirmTimeout].allSatisfy({$0.isFinite && $0>=0}),clickDelay<=10000,regionDelay<=10000,zoneDelay<=10000,poll>=1,poll<=2000,resultTimeout>=100,resultTimeout<=30000,searchTimeout>=1000,searchTimeout<=43_200_000,cancelTimeout>=1000,cancelTimeout<=43_200_000,confirmTimeout>=100,confirmTimeout<=30000 else {throw fail("지연·탐색 제한·좌석 수 범위를 확인하세요.")}
        if cancel {try rect("Zone1");try point("CancelDone"); if twoZones {try point("Zone1");try point("Zone2");try rect("Zone2")}}
        else {try rect("Seat");try point("Done");if useRegion{try point("Region")}}
        if scheduled {try point("Open")}
        if autoReady {try color("Ready")}
        if conflict {try color("ConflictA");try color("ConflictB");try point("Dismiss");guard spot("ConflictA").point != spot("ConflictB").point else{throw fail("경합 표식 A/B는 다른 위치여야 합니다.")}}
        if nextReady {try color("Success")}
        if follow {
            guard nextReady else{throw fail("후속 클릭 전 가격 화면 준비 색을 설정하세요.")}
            guard steps.count<=20 else{throw fail("후속 클릭은 최대 20개입니다.")}
            for s in steps where s.enabled {
                guard let p=s.target.point,p.x<size.width,p.y<size.height,s.delay.isFinite,s.timeout.isFinite,s.delay>=0,s.timeout>s.delay,s.timeout<=120000 else{throw fail("후속 클릭 좌표·대기·제한을 확인하세요.")}
                if s.wait {guard let p=s.probe.point,p.x<size.width,p.y<size.height,s.probe.rgb != nil else{throw fail("후속 단계의 대기 픽셀을 지정하세요.")}}
            }
        }
    }
}
struct ClockSample {
    var server:Double, mono:Double, rtt:Double, uncertainty:Double
    func deadline(_ target:Double)->Double {mono+target-server}
}
enum ClockParser {
    static func value(data:Data,dateHeader:String?,mode:String,path:String)->(Double,Double)? {
        var value:Any=String(data:data,encoding:.utf8)?.trimmingCharacters(in:.whitespacesAndNewlines) ?? ""
        if !path.isEmpty,mode != "date" {
            guard let json=try? JSONSerialization.jsonObject(with:data,options:.fragmentsAllowed) else{return nil}
            value=json
            for part in path.split(separator:".") {guard let v=(value as? [String:Any])?[String(part)] else{return nil};value=v}
        }
        let text=(value as? String) ?? (value as? NSNumber)?.stringValue ?? ""
        var n:Double?,precision=1.0
        if mode=="date" {let f=DateFormatter();f.locale=Locale(identifier:"en_US_POSIX");f.timeZone=TimeZone(secondsFromGMT:0);f.dateFormat="EEE, dd MMM yyyy HH:mm:ss zzz";n=f.date(from:dateHeader ?? "").map{$0.timeIntervalSince1970*1000+500};precision=1000}
        else if mode=="iso" {
            guard text.range(of:"(Z|[+-][0-9]{2}:[0-9]{2})$",options:.regularExpression) != nil else{return nil}
            if let dot=text.range(of:"\\.[0-9]+",options:.regularExpression){precision=pow(10,Double(max(0,3-(text[dot].count-1))))}else{precision=1000}
            let f=ISO8601DateFormatter();f.formatOptions=[.withInternetDateTime,.withFractionalSeconds];n=(f.date(from:text) ?? ISO8601DateFormatter().date(from:text)).map{$0.timeIntervalSince1970*1000}
        } else if mode=="milliseconds" || mode=="seconds" {n=Double(text);if mode=="seconds"{n=n.map{$0*1000};precision=1000}}
        guard let n=n,n.isFinite,(946684800000...4102444800000).contains(n) else{return nil};return(n,precision)
    }
    static func target(_ text:String)->Double? {
        let f=DateFormatter();f.locale=Locale(identifier:"en_US_POSIX");f.timeZone=TimeZone(secondsFromGMT:9*3600);f.dateFormat="yyyy-MM-dd HH:mm:ss.SSS";f.isLenient=false
        guard let d=f.date(from:text),f.string(from:d)==text else{return nil};return d.timeIntervalSince1970*1000
    }
}
struct Pixels {
    let bytes:UnsafePointer<UInt8>,width:Int,height:Int,stride:Int
    func color(_ p:CGPoint)->UInt32? {guard p.x.isFinite,p.y.isFinite,p.x>=0,p.y>=0,p.x<Double(width),p.y<Double(height) else{return nil};let x=Int(p.x),y=Int(p.y);let i=y*stride+x*4;return UInt32(bytes[i+2])<<16|UInt32(bytes[i+1])<<8|UInt32(bytes[i])}
    static func matches(_ a:UInt32,_ b:UInt32,_ tolerance:Int)->Bool {
        for shift in [0,8,16] {let left=Int((a >> shift) & 255);let right=Int((b >> shift) & 255);if abs(left-right)>tolerance{return false}};return true
    }
    func matches(_ s:Spot,_ t:Int)->Bool {guard let p=s.point,let a=color(p),let b=s.rgb else{return false};return Self.matches(a,b,t)}
    func find(a:CGPoint,b:CGPoint,rgb:UInt32,tolerance:Int,excluded:[CGPoint],gap:Int)->CGPoint? {
        guard a.x>=0,a.y>=0,b.x<Double(width),b.y<Double(height),a.x<=b.x,a.y<=b.y else{return nil}
        for y in Int(a.y)...Int(b.y) { for x in Int(a.x)...Int(b.x) {
            let p=CGPoint(x:x,y:y)
            if let c=color(p),Self.matches(c,rgb,tolerance),!excluded.contains(where:{abs($0.x-p.x)<=Double(gap)&&abs($0.y-p.y)<=Double(gap)}) {
                // Choose a short same-color run's center; never add an unchecked offset into a gap.
                var end=x
                while end<min(x+gap,Int(b.x)),let c=color(CGPoint(x:end+1,y:y)),Self.matches(c,rgb,tolerance){end+=1}
                return CGPoint(x:(x+end)/2,y:y)
            }
        }}
        return nil
    }
}
func coreTests() throws -> [String] {
    func check(_ b:Bool,_ s:String)throws{if !b{throw fail("TEST FAILED: "+s)}}
    try check(ClockParser.target("2026-12-05 18:00:00.000") != nil,"KST target")
    try check(ClockParser.target("2026-02-30 18:00:00.000")==nil,"invalid date")
    try check(ClockParser.value(data:Data("{\"data\":{\"now\":1800000000123}}".utf8),dateHeader:nil,mode:"milliseconds",path:"data.now")?.0==1800000000123,"JSON clock")
    try check(ClockParser.value(data:Data("NaN".utf8),dateHeader:nil,mode:"milliseconds",path:"")==nil,"invalid clock")
    try check(ClockParser.value(data:Data("2026-12-05T18:00:00".utf8),dateHeader:nil,mode:"iso",path:"")==nil,"ISO needs timezone")
    try check(ClockParser.value(data:Data("2026-12-05T18:00:00Z".utf8),dateHeader:nil,mode:"iso",path:"")?.1==1000,"whole-second ISO precision")
    try check(ClockParser.value(data:Data("2026-12-05T18:00:00.12Z".utf8),dateHeader:nil,mode:"iso",path:"")?.1==10,"fractional ISO precision")
    try check(ClockSample(server:1000,mono:20,rtt:2,uncertainty:1).deadline(1500)==520,"monotonic deadline")
    var bytes=[UInt8](repeating:0,count:16*8*4)
    for y in 2...4 {for x in [2,3,9,10]{bytes[(y*16+x)*4+1]=200}}
    try bytes.withUnsafeBufferPointer { raw in
        let p=Pixels(bytes:raw.baseAddress!,width:16,height:8,stride:64)
        let first=p.find(a:.zero,b:CGPoint(x:15,y:7),rgb:0x00c800,tolerance:0,excluded:[],gap:2)
        try check(first==CGPoint(x:2,y:2),"first seat")
        try check(p.find(a:.zero,b:CGPoint(x:15,y:7),rgb:0x00c800,tolerance:0,excluded:[first!],gap:3)==CGPoint(x:9,y:2),"exclude rejected seat")
        try check(p.find(a:.zero,b:CGPoint(x:30,y:7),rgb:0x00c800,tolerance:0,excluded:[],gap:2)==nil,"bounds guard")
    }
    var c=Settings();c.useRegion=false;c.spots=["SeatTL":Spot(x:"1",y:"1"),"SeatBR":Spot(x:"20",y:"20"),"SeatColor":Spot(color:"#00c800"),"Done":Spot(x:"30",y:"30")]
    try c.validate(size:CGSize(width:100,height:100),cancel:false)
    c.follow=true
    var rejected=false;do{try c.validate(size:CGSize(width:100,height:100),cancel:false)}catch{rejected=true};try check(rejected,"follow gate required")
    return ["KST/invalid date","JSON/invalid/ISO clock","monotonic deadline","pixel search/exclusion/bounds","profile/follow gate"]
}
