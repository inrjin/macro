import AppKit

// Deterministic fixtures call the production state machine. They never post input,
// capture a screen, register hotkeys, or read/write the user's saved settings.
@MainActor func runtimeTests() throws -> [String] {
    func check(_ ok:Bool,_ name:String)throws{if !ok{throw fail("TEST FAILED: "+name)}}
    func spot(_ x:Int,_ y:Int,_ color:String="")->Spot{Spot(x:String(x),y:String(y),color:color)}
    func config()->Settings {
        var c=Settings();c.useRegion=false;c.poll=1;c.gap=3;c.regionDelay=10;c.zoneDelay=10
        c.spots=["SeatTL":spot(1,6),"SeatBR":spot(24,16),"SeatColor":spot(4,8,"#00C800"),
                 "Done":spot(30,20),"CancelDone":spot(30,20),"Open":spot(1,20),"Region":spot(2,20),
                 "ConflictA":spot(1,1,"#FF0000"),"ConflictB":spot(2,1,"#0000FF"),"Dismiss":spot(3,20),
                 "Success":spot(29,1,"#FFFF00"),"Ready":spot(28,1,"#FFFFFF"),
                 "Zone1":spot(4,20),"Zone2":spot(5,20)]
        for z in ["Zone1","Zone2"] {c.spots[z+"TL"]=c.spots["SeatTL"];c.spots[z+"BR"]=c.spots["SeatBR"];c.spots[z+"Color"]=c.spots["SeatColor"]}
        return c
    }
    func paint(_ bytes:inout [UInt8],_ x:Int,_ y:Int,_ rgb:UInt32){let i=(y*32+x)*4;bytes[i]=UInt8(rgb&255);bytes[i+1]=UInt8((rgb>>8)&255);bytes[i+2]=UInt8((rgb>>16)&255);bytes[i+3]=255}
    func screen()->[UInt8]{var b=[UInt8](repeating:0,count:32*24*4);paint(&b,4,8,0x00C800);paint(&b,16,8,0x00C800);return b}
    func engine()->CoordinateEngine {let e=CoordinateEngine(testing:true);e.cfg=config();e.settings=e.cfg;e.running=true;e.phase="SEARCH";e.frameID=1;e.searchEnd=1_000_000;e.deadline=1_000_000;return e}
    func tick(_ e:CoordinateEngine,_ b:[UInt8],_ now:Double) {b.withUnsafeBufferPointer{e.process(Pixels(bytes:$0.baseAddress!,width:32,height:24,stride:128),now)}}
    var results:[String]=[]
    do {
        let e=engine();var clicks:[CGPoint]=[];e.testClick={clicks.append($0)};let b=screen()
        e.phase="REGION";tick(e,b,10);tick(e,b,25)
        try check(clicks.count==1 && e.candidate==nil,"pre-region frame cannot select a seat")
        e.frameID+=1;tick(e,b,26)
        try check(e.phase=="CLICK" && e.candidate==CGPoint(x:4,y:8),"new region frame can select")
        results.append("region requires a new frame")
    }
    do {
        let e=engine();e.cfg.conflict=true;var clicks:[CGPoint]=[];e.testClick={clicks.append($0)};var b=screen()
        tick(e,b,10);tick(e,b,12)
        try check(e.phase=="CONFIRM" && clicks==[CGPoint(x:4,y:8)],"first seat input")
        paint(&b,1,1,0xFF0000);paint(&b,2,1,0x0000FF);e.frameID+=1;tick(e,b,14)
        try check(e.phase=="DISMISS" && e.rejected[1]==[CGPoint(x:4,y:8)] && clicks.last==CGPoint(x:3,y:20),"conflict rejected and dismissed")
        paint(&b,1,1,0);paint(&b,2,1,0);e.frameID+=1;tick(e,b,16);tick(e,b,18);tick(e,b,20)
        try check(clicks.last==CGPoint(x:16,y:8) && clicks.count==3,"retry must use another seat")
        results.append("conflict dismissal and different-seat retry")
    }
    do {
        let e=engine();var clicks:[CGPoint]=[];e.testClick={clicks.append($0)};var b=screen()
        tick(e,b,10);tick(e,b,12);paint(&b,4,8,0xCC0000)
        tick(e,b,14);try check(e.phase=="CONFIRM","same capture cannot acknowledge input")
        e.frameID+=1;tick(e,b,16);try check(e.phase=="COMPLETE","changed color acknowledged")
        paint(&b,4,8,0x00C800);e.frameID+=1;tick(e,b,18)
        try check(!e.running && clicks.count==1,"rolled-back selection must not click complete")
        results.append("fresh confirmation and selection rollback")
    }
    do {
        let e=engine();e.cancel=true;e.cfg.twoZones=true;var clicks:[CGPoint]=[];e.testClick={clicks.append($0)}
        let empty=[UInt8](repeating:0,count:32*24*4)
        tick(e,empty,10);tick(e,empty,12)
        try check(e.zone==2 && clicks==[CGPoint(x:5,y:20)],"empty zone alternates")
        e.frameID+=1;tick(e,screen(),24);tick(e,screen(),26)
        try check(e.phase=="CONFIRM" && clicks.last==CGPoint(x:4,y:8),"cancellation seat selected")
        let single=engine();single.cancel=true;single.testClick={_ in};tick(single,empty,10);tick(single,empty,200)
        try check(single.running && single.phase=="SEARCH","single-zone cancellation keeps waiting")
        tick(single,empty,1_000_001);try check(!single.running,"cancellation timeout")
        results.append("cancellation modes and timeout")
    }
    do {
        let e=engine();e.cfg.nextReady=true;e.cfg.follow=true
        e.cfg.steps=[ClickStep(name:"quantity",target:spot(6,20),delay:0,timeout:1000),
                     ClickStep(name:"continue",target:spot(7,20),delay:5,wait:true,probe:spot(27,1,"#FFFFFF"),timeout:1000)]
        var clicks:[CGPoint]=[];e.testClick={clicks.append($0)}
        var b=screen();paint(&b,4,8,0xCC0000);paint(&b,29,1,0xFFFF00)
        e.selected=[CGPoint(x:4,y:8)];e.phase="COMPLETE"
        tick(e,b,10);tick(e,b,12);try check(e.phase=="RESULT","stale success pixel cannot pass")
        e.frameID+=1;tick(e,b,14);tick(e,b,16);tick(e,b,18);tick(e,b,20)
        try check(clicks==[CGPoint(x:30,y:20),CGPoint(x:6,y:20)],"follow only after price gate")
        tick(e,b,30);try check(clicks.count==2,"step waits for its own pixel")
        paint(&b,27,1,0xFFFFFF);e.frameID+=1;tick(e,b,32);tick(e,b,34)
        try check(!e.running && clicks.last==CGPoint(x:7,y:20),"ordered steps stop after completion")
        results.append("price gate and ordered follow-up clicks")
    }
    do {
        let e=engine();var clicks:[CGPoint]=[];e.testClick={clicks.append($0)};var b=screen()
        e.phase="HUMAN";e.cfg.autoReady=true;paint(&b,28,1,0xFFFFFF)
        tick(e,b,10);tick(e,b,100);try check(e.phase=="HUMAN","initial ready match must not resume")
        paint(&b,28,1,0);tick(e,b,110);paint(&b,28,1,0xFFFFFF);e.frameID+=1
        tick(e,b,120);try check(e.phase=="HUMAN","ready debounce");tick(e,b,140)
        try check(e.phase=="SEARCH" && clicks.isEmpty,"CAPTCHA readiness resumes without solving it")
        e.stop("test stop");tick(e,b,200)
        try check(clicks.isEmpty,"stop prevents further input")
        results.append("manual-CAPTCHA readiness and stop")
    }
    do {
        let e=CoordinateEngine(testing:true);e.settings=config();e.verified=true
        e.bound=BrowserWindow(id:1,pid:1,rect:CGRect(x:0,y:0,width:32,height:24))
        var clicks:[CGPoint]=[];e.testClick={clicks.append($0)}
        e.settings.saleTime="2026-12-05 18:00:00.000";e.settings.clockURL="https://example.invalid/time"
        let target=ClockParser.target(e.settings.saleTime)!,now=monoMS()
        e.clock=ClockSample(server:target-1000,mono:now,rtt:1,uncertainty:0.5)
        e.clockIdentity=e.settings.clockURL+"|milliseconds|";e.start(cancel:false,scheduled:true)
        try check(e.running && e.phase=="OPENING" && clicks.isEmpty,"schedule arms without clicking")
        e.due=monoMS()-1;e.tick()
        try check(e.phase=="HUMAN" && clicks==[CGPoint(x:1,y:20)],"scheduled opening exactly once")
        e.tick();try check(clicks.count==1,"opening does not repeat")
        e.stop("test");e.settings.clockURL="https://example.invalid/changed";e.start(cancel:false,scheduled:true)
        try check(!e.running,"changed API invalidates measurement")
        results.append("scheduled single opening and changed-API guard")
    }
    return results
}
