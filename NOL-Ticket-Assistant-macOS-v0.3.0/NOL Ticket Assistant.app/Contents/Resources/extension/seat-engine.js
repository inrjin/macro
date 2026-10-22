import {validateSeatProfile, pixelCandidates, normalize} from './seat-core.js';
import {allowedUrl, samePage} from './core.js';
export const seatUrl = value => {
  if (allowedUrl(value)) return true;
  try {const u=new URL(value);return u.protocol==='file:' && u.pathname.endsWith('/rehearsal-competition.html') && !u.host;}catch{return false;}
};
export class SeatEngine {
  constructor(changed=()=>{}) {this.changed=changed;this.run=null;this.input=Promise.resolve();}
  status() {const r=this.run;return r?{id:r.id,tabId:r.tabId,phase:r.phase,attempt:r.attempt,count:r.count,message:r.message,startedAt:r.startedAt,region:r.profile.entrance?.enabled?(r.profile.entrance.name||'지정 구역'):null}:null;}
  busy() {return !!this.run && !['DONE','STOPPED'].includes(this.run.phase);}
  publish() {Promise.resolve(this.changed(this.status())).catch(()=>{});}
  async message(type,extra={},frameId=0) {
    const r=await chrome.tabs.sendMessage(this.run.tabId,{type,...extra},{frameId});
    if(!r?.ok)throw new Error(r?.error||'좌석 화면에 연결되지 않았습니다. 새로고침하세요.');return r;
  }
  async command(method,params={}) {return chrome.debugger.sendCommand({tabId:this.run.tabId},method,params);}
  async start(tabId,input) {
    if(this.busy())throw new Error('이미 좌석 작업이 진행 중입니다.');
    const profile=validateSeatProfile(input),tab=await chrome.tabs.get(tabId);
    if(!seatUrl(tab.url))throw new Error('NOL 좌석 화면 또는 제공된 연습 화면에서 실행하세요.');
    const r=this.run={id:crypto.randomUUID(),tabId,url:tab.url,profile,phase:'WAIT_FOCUS',attempt:0,index:0,count:0,startedAt:Date.now(),attached:false};
    this.publish();
    try {
      await chrome.debugger.attach({tabId},'1.3');r.attached=true;
      await this.command('Page.enable');
      r.frameId=0;
      r.timer=setTimeout(()=>this.stop('브라우저 포커스를 기다리다 중지했습니다.'),10000);
      await this.message('seatAwaitFocus',{runId:r.id});
      return {ok:true};
    }catch(e){await this.stop(e.message);throw e;}
  }
  async go(m,sender) {
    const r=this.run;
    if(!r||r.phase!=='WAIT_FOCUS'||r.id!==m.runId||sender.tab?.id!==r.tabId||sender.frameId!==0)return {ok:false};
    try {await this.focused();}catch{return {ok:false};}
    clearTimeout(r.timer);
    try {
      if(r.profile.entrance?.enabled) {
        const e=r.profile.entrance;
        await this.waitStage(r,'entry',e.frameUrl,{blockedSelector:e.blockedSelector,readySelector:e.readySelector,readyText:e.readyText},e.timeoutMs);
      }else await this.prepareSeats(r);
      return {ok:true};
    }catch(e){await this.stop(e.message);return {ok:false,error:e.message};}
  }
  async frameFor(url,r=this.run,optional=false) {
    if(!url||samePage(url,r.url))return 0;
    if(!allowedUrl(url))throw new Error('감지 프레임은 허용된 NOL 주소여야 합니다.');
    const frames=(await chrome.webNavigation.getAllFrames({tabId:r.tabId})).filter(f=>samePage(f.url,url));
    if(frames.length>1)throw new Error('감지 프레임이 여러 개입니다. URL을 특정하세요.');
    if(!frames.length){if(optional)return null;throw new Error('감지 프레임이 아직 없습니다.');}
    return frames[0].frameId;
  }
  async waitStage(r,name,url,condition,timeoutMs) {
    if(!this.alive(r))return;
    await this.clearStage(r);
    r.phase=name==='entry'?'WAIT_ENTRY':'WAIT_MAP';
    const stage=r.stage={name,url:url||r.url,condition,token:crypto.randomUUID(),frameId:null,bound:false,binding:false,handling:false};
    r.timer=setTimeout(()=>this.stop(name==='entry'?'사용자 확인/구역 선택 화면을 기다리다 중지했습니다.':'지정 구역의 지도 로딩을 확인하지 못했습니다. 다른 구역으로 이동하지 않습니다.'),timeoutMs);
    // Only used while a new frame/content script is still being created; removed once bound.
    stage.retry=setInterval(()=>this.bindStage(r).catch(e=>this.stop(e.message)),250);
    this.publish();await this.bindStage(r);
  }
  async clearStage(r) {
    clearTimeout(r.timer);const stage=r.stage;r.stage=null;
    if(stage){clearInterval(stage.retry);if(stage.frameId!==null)await chrome.tabs.sendMessage(r.tabId,{type:'seatStageClear'},{frameId:stage.frameId}).catch(()=>{});}
  }
  async bindStage(r=this.run) {
    const s=r?.stage;if(!r||!s||s.bound||s.binding||s.handling||!this.alive(r))return;
    s.binding=true;const token=s.token;s.bindingToken=token;
    try{
      const frameId=await this.frameFor(s.url,r,true);
      if(frameId===null||r.stage!==s||!this.alive(r))return;
      s.frameId=frameId;
      const reply=await chrome.tabs.sendMessage(r.tabId,{type:'seatStageWatch',runId:r.id,token,condition:s.condition},{frameId}).catch(()=>null);
      if(r.stage!==s||s.token!==token)return;
      if(reply?.ok){s.bound=true;clearInterval(s.retry);}else if(reply)throw new Error(reply.error||'진입 감지 조건 오류');
    }finally{if(s.bindingToken===token)s.binding=false;}
  }
  async documentReady(sender) {
    const r=this.run,s=r?.stage;
    if(!this.busy()||sender.tab?.id!==r.tabId)return {ok:false};
    if(r.guard&&samePage(sender.url,r.guard.url)){
      r.guard.frameId=sender.frameId;
      await this.message('seatBlockWatch',{runId:r.id,selector:r.profile.entrance.blockedSelector},sender.frameId);
    }
    if(!s||!samePage(sender.url,s.url)||s.handling)return {ok:false};
    s.token=crypto.randomUUID();s.bound=false;s.binding=false;
    await this.bindStage(r);return {ok:true};
  }
  async gateBlocked(m,sender) {
    const r=this.run;
    if(!this.busy()||m.runId!==r.id||sender.tab?.id!==r.tabId||sender.frameId!==r.guard?.frameId||!samePage(sender.url,r.guard.url))return {ok:false};
    await this.stop('사용자 확인 창이 다시 나타나 자동 클릭을 중지했습니다. 직접 확인하세요.');return {ok:true};
  }
  async stageReady(m,sender) {
    const r=this.run,s=r?.stage;
    if(!this.alive(r)||!s||m.runId!==r.id||m.token!==s.token||sender.tab?.id!==r.tabId||sender.frameId!==s.frameId||!samePage(sender.url,s.url)||s.handling)return {ok:false};
    s.handling=true;
    try{
      if(m.error)throw new Error(m.error);
      await this.focused();
      const fresh=await this.message('seatStageCheck',{condition:s.condition},s.frameId);
      if(!fresh.ready){s.handling=false;s.bound=false;await this.bindStage(r);return {ok:false};}
      if(!this.alive(r)||r.stage!==s)return {ok:false};
      if(s.name==='entry') {
        const e=r.profile.entrance;
        r.guard={frameId:s.frameId,url:s.url};
        await this.message('seatBlockWatch',{runId:r.id,selector:e.blockedSelector},s.frameId);
        // Freeze viewport geometry at entry, after the user's typing is complete.
        await this.message('seatPrepare',{selector:null});
        await this.waitStage(r,'map',e.mapFrameUrl||e.frameUrl,{blockedSelector:e.mapBlockedSelector||e.blockedSelector,readySelector:e.mapReadySelector,readyText:e.mapReadyText},e.mapTimeoutMs);
        await this.step(e.region,r,'WAIT_MAP');
      }else{
        await this.input;await this.clearStage(r);
        if(this.alive(r))await this.prepareSeats(r);
      }
      return {ok:true};
    }catch(e){await this.stop(e.message);return {ok:false,error:e.message};}
  }
  async prepareSeats(r) {
      if(!this.alive(r))return;
      r.phase='PREPARING';this.publish();
      r.frameId=await this.frameFor(r.profile.outcomeFrameUrl,r);
      const prepared=await this.message('seatPrepare',{selector:r.profile.mode==='dom'?r.profile.candidateSelector:null});
      if(r.profile.mode==='pixels') {
        const g=prepared.geometry;
        let roi=r.profile.roi;
        if(r.profile.mapAnchor?.selector){roi=await this.mapBounds(r,g);r.mapROI=roi;}
        if(!roi||roi.width<1||roi.height<1||roi.width*roi.height>2000000)throw new Error('현재 좌석 지도 범위가 비어 있거나 200만 CSS 픽셀을 넘습니다.');
        if(roi.x+roi.width>g.width||roi.y+roi.height>g.height)throw new Error('좌석 검색 범위가 현재 화면 밖입니다.');
        // Capture the existing viewport without a CDP clip: clipping can resize emulated viewports.
        const shot=await this.command('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
        const blob=await (await fetch('data:image/png;base64,'+shot.data)).blob();const bitmap=await createImageBitmap(blob);
        const canvas=new OffscreenCanvas(Math.ceil(roi.width),Math.ceil(roi.height)),ctx=canvas.getContext('2d',{willReadFrequently:true});
        // Chrome may crop blank space below a short document. A shorter PNG is not Y scaling.
        const scaleX=bitmap.width/g.width,scaleY=scaleX;
        if((roi.y+roi.height)*scaleY>bitmap.height+1){bitmap.close();throw new Error('현재 캡처에 좌석 지도 전체가 포함되지 않았습니다. 화면을 조정하세요.');}
        ctx.drawImage(bitmap,roi.x*scaleX,roi.y*scaleY,roi.width*scaleX,roi.height*scaleY,0,0,canvas.width,canvas.height);bitmap.close();
        r.points=pixelCandidates(ctx.getImageData(0,0,canvas.width,canvas.height).data,canvas.width,canvas.height,{...r.profile,roi});r.count=r.points.length;
      }else r.count=prepared.count;
      if(!r.count)throw new Error('지정한 범위에 좌석 후보가 없습니다.');
      if(!this.alive(r))return {ok:false};
      await this.next(r);
  }
  async mapBounds(r,geometry) {
    const binding=r.profile.mapAnchor,frameId=await this.frameFor(binding.frameUrl,r);
    const info=await this.message('seatBindingRect',{binding},frameId);
    const a=await this.viewportPoint({x:info.rect.x,y:info.rect.y},binding.frameUrl,info.viewport,false);
    const b=await this.viewportPoint({x:info.rect.x+info.rect.width,y:info.rect.y+info.rect.height},binding.frameUrl,info.viewport,false);
    const x=Math.max(0,a.x),y=Math.max(0,a.y);
    const roi={x,y,width:Math.min(geometry.width,b.x)-x,height:Math.min(geometry.height,b.y)-y};
    r.mapGeometry=geometry;return roi;
  }
  alive(r) {return !!r && r===this.run && !['DONE','STOPPED'].includes(r.phase);}
  async focused() {
    const r=this.run,tab=await chrome.tabs.get(r.tabId),win=await chrome.windows.get(tab.windowId);
    if(!tab.active||!win.focused||!samePage(tab.url,r.url))throw new Error('실행 탭·주소·창 포커스가 바뀌었습니다.');
  }
  async click(point,r=this.run) {
    if(!this.alive(r))return;
    const task=(async()=>{
      await this.command('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,x:point.x,y:point.y});
      // Always release an issued press, including when the user stopped in between.
      await this.command('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,x:point.x,y:point.y});
    })();
    this.input=task;await task;
  }
  async viewportPoint(point,frameUrl,viewport,hitCheck=true) {
    const r=this.run;if(!frameUrl||samePage(frameUrl,r.url))return point;
    const frames=await chrome.webNavigation.getAllFrames({tabId:r.tabId});
    const matches=frames.filter(f=>samePage(f.url,frameUrl));
    if(matches.length!==1)throw new Error('클릭 프레임을 하나로 특정하지 못했습니다.');
    let child=matches[0],mapped=point,size=viewport;
    while(child.frameId!==0){
      const parent=frames.find(f=>f.frameId===child.parentFrameId);
      if(!parent||!seatUrl(parent.url))throw new Error('중간 프레임에 연결할 수 없습니다.');
      const token=crypto.randomUUID();
      // Match the actual child WindowProxy, including cross-origin and nested frames.
      await this.message('seatFrameLocate',{token,point:mapped,viewport:size,hitCheck},parent.frameId);
      await this.message('seatFrameProbe',{token},child.frameId);
      const info=await this.message('seatFrameResult',{token},parent.frameId);
      mapped=info.point;size=info.viewport;child=parent;
    }
    return mapped;
  }

  async step(step,r=this.run,phase=r.phase) {
    if(step.delayMs)await new Promise(resolve=>setTimeout(resolve,step.delayMs));
    if(!this.alive(r)||r.phase!==phase)return;
    await this.focused();let point;
    if(step.selector&&(step.relative||step.frameUrl)){
      const frameId=await this.frameFor(step.frameUrl,r);
      const info=await this.message('seatBindingPoint',{binding:step},frameId);
      point=await this.viewportPoint(info.point,step.frameUrl,info.viewport);
    }else point=(await this.message('seatPoint',{step})).point;
    if(this.alive(r)&&r.phase===phase)await this.click(point,r);
  }
  async next(r) {
    if(!this.alive(r))return;
    if(r.attempt>=r.profile.maxAttempts)throw new Error('설정한 최대 시도 횟수에 도달했습니다.');
    await this.focused();
    if(r.mapROI){
      const current=await this.mapBounds(r,r.mapGeometry);
      if(Object.keys(current).some(k=>Math.abs(current[k]-r.mapROI[k])>.5))throw new Error('좌석 지도 위치/크기가 바뀌어 준비한 후보를 중지했습니다. 다시 시작하세요.');
    }
    let point;
    while(r.index<r.count&&!point) {
      const index=r.index++;
      point=r.points?(await this.message('seatPoint',{step:r.points[index]})).point:(await this.message('seatCandidate',{index})).point;
    }
    if(!point)throw new Error('준비한 좌석 후보를 모두 확인했습니다.');
    if(!this.alive(r))return;
    r.attempt++;r.phase='WAIT_OUTCOME';
    await this.message('seatWatch',{profile:r.profile,runId:r.id,attempt:r.attempt},r.frameId);
    if(!this.alive(r))return;
    r.timer=setTimeout(()=>this.stop('좌석 결과 응답이 없어 중지했습니다. 선택 여부를 직접 확인하세요.'),r.profile.timeoutMs);
    await this.click(point,r);
    if(this.alive(r)&&r.phase==='WAIT_OUTCOME'&&r.profile.commit)await this.step(r.profile.commit,r);
    this.publish();
  }
  async outcome(m,sender) {
    const r=this.run;
    if(!r||r.id!==m.runId||r.attempt!==m.attempt||r.phase!=='WAIT_OUTCOME'||sender.tab?.id!==r.tabId||sender.frameId!==r.frameId)return {ok:false};
    if(m.kind==='unknown'){await this.stop('확인되지 않은 안내가 나타났습니다. '+(m.error||''));return {ok:true};}
    if(!['success','conflict'].includes(m.kind))return {ok:false};
    r.phase=m.kind==='success'?'AFTER':'RECOVERING';clearTimeout(r.timer);
    try{
      await this.input;if(!this.alive(r))return {ok:false};
      if(m.kind==='success') {
        for(const step of r.profile.after){await this.step(step,r);if(!this.alive(r))return {ok:false};}
        await this.stop('설정한 성공 조건을 확인했습니다. 후속 클릭 완료.', 'DONE');
      }else{
        await this.step(r.profile.dismiss,r);
        await this.message('seatWaitHidden',{selector:r.profile.conflictSelector,timeoutMs:r.profile.timeoutMs},r.frameId);
        await this.next(r);
      }
    }catch(e){await this.stop(e.message);}
    return {ok:true};
  }
  async dialog(source,params) {
    const r=this.run;if(!r||source.tabId!==r.tabId||!this.alive(r))return;
    // Accept ONLY a precisely configured rejection alert; never accept payment/other prompts.
    if(r.phase!=='WAIT_OUTCOME'||params.type!=='alert'||!r.profile.alertText||normalize(params.message)!==r.profile.alertText){await this.stop('알 수 없는 브라우저 알림입니다. 직접 확인하세요.');return;}
    r.phase='RECOVERING';clearTimeout(r.timer);
    try{
      // Do not queue behind mouseReleased: that command may be blocked by this dialog.
      await this.command('Page.handleJavaScriptDialog',{accept:true});
      await this.input;if(!this.alive(r))return;
      await this.message('seatOutcomeClear',{},r.frameId);
      await this.next(r);
    }catch(e){await this.stop(e.message);}
  }
  async stop(message='사용자가 중지했습니다.',phase='STOPPED') {
    const r=this.run;if(!r||!this.alive(r))return;
    r.phase=phase;r.message=message;clearTimeout(r.timer);
    await this.clearStage(r);
    this.publish();
    await chrome.tabs.sendMessage(r.tabId,{type:'seatClear'}).catch(()=>{});
    // Bound cleanup so a blocking unknown dialog never prevents the stop UI.
    if(r.attached){r.attached=false;await chrome.debugger.detach({tabId:r.tabId}).catch(()=>{});}
  }
}
