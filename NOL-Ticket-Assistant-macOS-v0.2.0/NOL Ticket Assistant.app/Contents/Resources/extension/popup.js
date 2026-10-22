const $ = id => document.getElementById(id);
const fields = ['url','buttonSelector','buttonText','saleAt','timeoutMinutes','autoMacro','seatDriver','readyUrl','readySelector','readyText','browser'];
const defaults = {url:'', buttonSelector:'#booking-aside-container button', buttonText:'예매하기', saleAt:'', timeoutMinutes:30, autoMacro:false, seatDriver:'browser', readyUrl:'', readySelector:'', readyText:'', browser:navigator.userAgent.includes('Edg/') ? 'msedge' : 'chrome'};
const request = async m => {const r = await chrome.runtime.sendMessage(m); if (r?.ok === false) throw new Error(r.error || '작업을 완료하지 못했습니다. 최근 기록을 확인하세요.'); return r;};
function config() { return Object.fromEntries(fields.map(id => [id, id==='autoMacro' ? $(id).checked : id==='timeoutMinutes' ? Number($(id).value) : $(id).value.trim()])); }
async function tab() { return (await chrome.tabs.query({active:true,currentWindow:true}))[0]; }
function clockText(c) { $('clockInfo').textContent = c ? `참고 오프셋 ${(c.offsetMs/1000).toFixed(3)}초 · 추정 오차 ±${Math.ceil(c.uncertaintyMs)}ms · ${new Date(c.sampledAt).toLocaleTimeString()} 측정. 참고용이며 실행 시각은 PC 시계 기준입니다.` : 'Date 헤더는 초 단위이며 예매 서버와 다를 수 있습니다. 시각 측정은 참고용이고 실행 시각은 PC 시계 기준입니다.'; }
async function refresh() {
  const r = await request({type:'status'}), c = {...defaults,...r.config};
  $('profile').value=JSON.stringify(r.profile||{mode:'dom',candidateSelector:'',successSelector:'',alertText:'',maxAttempts:12,timeoutMs:4000,after:[]},null,2);
  for (const id of fields) {if (id==='autoMacro') $(id).checked=c[id]; else $(id).value=c[id];}
  $('state').textContent = r.seatRun && !['DONE','STOPPED'].includes(r.seatRun.phase) ? '좌석 '+r.seatRun.phase : {ARMED:'버튼 대기',WAIT_READY:'좌석 준비 대기',STARTING:'매크로 연결',DONE:'완료',STOPPED:'중지'}[r.run?.phase] || '설정';
  $('extensionId').textContent='확장 ID: '+r.extensionId;
  $('logs').textContent=(r.logs||[]).slice().reverse().map(l=>new Date(l.time).toLocaleTimeString()+' '+l.message).join('\n'); clockText(r.clock);
}
function bind(id, fn) { $(id).addEventListener('click',async()=>{ $(id).disabled=true; $('message').textContent=''; try {await fn();}catch(e){$('message').textContent=e.message;}finally{$(id).disabled=false;} }); }
bind('useTab',async()=>{$('url').value=(await tab()).url;});
bind('save',async()=>{await request({type:'save',config:config()});$('message').textContent='설정을 저장했습니다.';});
for(const [id,kind] of [['pickButton','button'],['pickReady','ready']]) bind(id,async()=>{await request({type:'save',config:config()});await request({type:'pick',kind,tabId:(await tab()).id});window.close();});
bind('clock',async()=>{const r=await request({type:'clock',url:$('url').value});clockText(r.clock);});
bind('ping',async()=>{const r=await request({type:'ping'});$('message').textContent='Windows 브리지 응답: '+JSON.stringify(r);});
bind('stop',async()=>{await request({type:'stop'});await refresh();$('message').textContent='브라우저 대기 중지. 실행 중인 Windows 매크로는 F3으로 중지하세요.';});
bind('arm',async()=>{const c=config();await request({type:'save',config:c});await request({type:'arm',config:c,tabId:(await tab()).id});window.close();});
const seatProfile=()=>JSON.parse($('profile').value);
bind('practiceProfile',async()=>{$('profile').value=JSON.stringify({mode:'dom',candidateSelector:'.seat:not(.rejected)',successSelector:'#success',successText:'연습: 좌석 선택 성공',alertText:'연습: 이미 선택된 좌석입니다.',conflictSelector:'#conflictText',conflictText:'연습: 이미 선택된 좌석입니다.',dismiss:{selector:'#dismiss'},maxAttempts:100,timeoutMs:4000,after:[{selector:'#after1'},{selector:'#after2'}]},null,2);});
bind('seatSave',async()=>{await request({type:'seatSaveProfile',profile:seatProfile()});$('message').textContent='좌석 설정을 저장했습니다.';});
bind('seatStart',async()=>{await request({type:'seatStart',tabId:(await tab()).id,profile:seatProfile()});window.close();});
for(const [id,kind] of [['pickROI','roi'],['pickColor','color'],['pickCommit','commit'],['pickAfter','after']])bind(id,async()=>{await request({type:'seatPick',tabId:(await tab()).id,kind,profile:seatProfile()});window.close();});
bind('macConnect',async()=>{await request({type:'macConnect'});$('message').textContent='연결을 요청했습니다. Mac 앱 상태와 최근 기록을 확인하세요.';});
refresh().catch(e=>{$('message').textContent=e.message;});
