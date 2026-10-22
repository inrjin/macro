export const normalize = value => String(value || '').replace(/\s+/g, ' ').trim();
export function validateSeatProfile(input) {
  if(!input||typeof input!=='object')throw new Error('좌석 프로필을 먼저 지정하세요.');
  const p = structuredClone(input);
  if (!['dom','pixels'].includes(p.mode)) throw new Error('좌석 방식은 dom 또는 pixels여야 합니다.');
  if (p.mode === 'dom' && !p.candidateSelector?.trim()) throw new Error('선택 가능한 좌석의 CSS 선택자를 지정하세요.');
  if (p.mode === 'pixels') {
    if (!p.mapAnchor?.selector && (!p.roi || !['x','y','width','height'].every(k=>Number.isFinite(p.roi[k])) || p.roi.x<0 || p.roi.y<0 || p.roi.width<1 || p.roi.height<1 || p.roi.width*p.roi.height>2000000)) throw new Error('픽셀 검색 범위를 지정하세요 (최대 200만 CSS 픽셀).');
    if (!/^#[\da-f]{6}$/i.test(p.color || '')) throw new Error('좌석 색은 #RRGGBB 형식입니다.');
    p.tolerance ??= 3; p.minArea ??= 4; p.maxArea ??= 2500;
    if (!Number.isInteger(p.tolerance) || p.tolerance<0 || p.tolerance>64 || !Number.isInteger(p.minArea) || !Number.isInteger(p.maxArea) || p.minArea<1 || p.maxArea<p.minArea) throw new Error('색 오차/좌석 크기 범위를 확인하세요.');
  }
  if (!p.successSelector?.trim()) throw new Error('실제 선택 성공을 확인할 요소를 지정해야 합니다. 색 변화만으로 성공 처리하지 않습니다.');
  p.alertText = normalize(p.alertText); p.conflictText = normalize(p.conflictText);
  if (!p.alertText && !(p.conflictSelector && p.conflictText && p.dismiss)) throw new Error('확인된 선점 실패 알림 문구 또는 화면 안내창 조건을 지정하세요.');
  p.maxAttempts ??= 12; p.timeoutMs ??= 4000; p.after ??= [];
  if (!Number.isInteger(p.maxAttempts) || p.maxAttempts<1 || p.maxAttempts>100 || !Number.isInteger(p.timeoutMs) || p.timeoutMs<250 || p.timeoutMs>30000) throw new Error('시도 횟수는 1~100, 응답 제한은 250~30000ms입니다.');
  if (!Array.isArray(p.after) || p.after.length>20) throw new Error('후속 클릭은 최대 20개입니다.');
  if(p.entrance?.enabled) {
    const e=p.entrance;
    if(!e.blockedSelector?.trim()||!e.readySelector?.trim()||!e.mapReadySelector?.trim()||!e.region?.selector?.trim())throw new Error('캡챠/입력 창, 구역 선택 준비 요소, 지정 구역 클릭, 해당 구역의 지도 준비 요소를 모두 지정하세요.');
    if(e.region.kind==='canvas')throw new Error('Canvas 구역 내부의 확대·이동 구조 확인이 필요합니다.');
    e.timeoutMs??=180000;e.mapTimeoutMs??=10000;
    if(!Number.isInteger(e.timeoutMs)||e.timeoutMs<1000||e.timeoutMs>600000||!Number.isInteger(e.mapTimeoutMs)||e.mapTimeoutMs<250||e.mapTimeoutMs>60000)throw new Error('진입 대기는 1~600초, 지도 로딩 제한은 250~60000ms입니다.');
  }
  for (const step of [p.commit,p.dismiss,p.entrance?.enabled?p.entrance.region:null,...p.after].filter(Boolean)) {
    if (!step.selector && !(Number.isFinite(step.x)&&Number.isFinite(step.y)&&step.x>=0&&step.y>=0)) throw new Error('클릭 단계에는 CSS 선택자 또는 X/Y가 필요합니다.');
    if(step.relative&&(!step.selector||!['x','y'].every(k=>Number.isFinite(step.relative[k])&&step.relative[k]>=0&&step.relative[k]<=1)))throw new Error('구역 상대 좌표는 지도 요소 기준 0~1입니다.');
    if (step.delayMs!==undefined && (!Number.isFinite(step.delayMs)||step.delayMs<0||step.delayMs>30000)) throw new Error('후속 대기는 0~30000ms입니다.');
  }
  return p;
}
// Connected components keep multiple matching pixels of one seat from becoming multiple candidates.
export function pixelCandidates(rgba,width,height,p) {
  const rgb=[1,3,5].map(i=>parseInt(p.color.slice(i,i+2),16)), match=new Uint8Array(width*height), visited=new Uint8Array(width*height);
  for(let i=0;i<match.length;i++){const j=i*4;match[i]=rgba[j+3]>0 && rgb.every((c,k)=>Math.abs(rgba[j+k]-c)<=p.tolerance)?1:0;}
  const result=[], queue=new Int32Array(width*height);
  for(let start=0;start<match.length;start++){
    if(!match[start]||visited[start])continue;
    let head=0,tail=1,count=0,sx=0,sy=0;queue[0]=start;visited[start]=1;
    while(head<tail){const i=queue[head++],x=i%width,y=Math.floor(i/width);count++;sx+=x;sy+=y;
      for(const n of [x>0?i-1:-1,x+1<width?i+1:-1,y>0?i-width:-1,y+1<height?i+width:-1])if(n>=0&&match[n]&&!visited[n]){visited[n]=1;queue[tail++]=n;}
    }
    if(count<p.minArea||count>p.maxArea)continue;
    // Choose a matching pixel nearest the component centroid, never a hole in a seat outline.
    const cx=sx/count,cy=sy/count;let best=start,distance=Infinity;
    for(let k=0;k<tail;k++){const i=queue[k],d=(i%width-cx)**2+(Math.floor(i/width)-cy)**2;if(d<distance){best=i;distance=d;}}
    result.push({x:p.roi.x+(best%width+.5)*p.roi.width/width,y:p.roi.y+(Math.floor(best/width)+.5)*p.roi.height/height});
    if(result.length>=1000)break;
  }
  return result;
}
