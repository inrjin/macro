// The native host accepts a fixed command vocabulary. No shell or page code execution.
export function macLink(status,handle,report) {
  let port=null,timer=null;
  const browser=/Edg\//.test(globalThis.navigator?.userAgent||'')?'msedge':'chrome';
  const seen=new Set();
  async function push(){if(!port)return;try{port.postMessage({op:'status',state:{...await status(),browser,updatedAt:Date.now()}});}catch{}}
  function connect(){
    if(port){push();return {ok:true,connected:true};}
    const p=chrome.runtime.connectNative('com.nol_ticket_assistant.mac');port=p;
    p.onMessage.addListener(async m=>{
      if(m.op!=='appCommand'||!m.id||seen.has(m.id)||m.payload?.browser!==browser)return;
      seen.add(m.id);if(seen.size>100)seen.delete(seen.values().next().value);
      try{
        if(!['seatStart','stop','seatPick','arm'].includes(m.action))throw new Error('지원하지 않는 앱 명령입니다.');
        const [tab]=await chrome.tabs.query({active:true,lastFocusedWindow:true});
        if(!tab)throw new Error('사용할 브라우저 탭을 전면에 두세요.');
        await handle({type:m.action,tabId:tab.id,...m.payload});
      }catch(e){await report('Mac 앱 요청 실패: '+e.message);}
      push();
    });
    p.onDisconnect.addListener(()=>{const error=chrome.runtime.lastError?.message; if(port===p){port=null;clearInterval(timer);timer=null;}if(error)report('Mac 앱 연결: '+error);});
    p.postMessage({op:'ping'});timer=setInterval(push,2000);push();
    return {ok:true,connected:true};
  }
  return {connect,push};
}
