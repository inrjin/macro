import test from 'node:test';import assert from 'node:assert/strict';
import {validateSeatProfile,pixelCandidates} from '../extension/seat-core.js';
const p={mode:'dom',candidateSelector:'.seat',successSelector:'#success',alertText:'already chosen'};
test('requires affirmative outcome and exact rejection conditions',()=>{assert.throws(()=>validateSeatProfile({}));assert.throws(()=>validateSeatProfile({...p,successSelector:''}));assert.throws(()=>validateSeatProfile({...p,alertText:''}));assert.throws(()=>validateSeatProfile({...p,maxAttempts:101}));assert.equal(validateSeatProfile(p).maxAttempts,12);});
test('connected components group one colored seat, not each pixel',()=>{const data=new Uint8ClampedArray(20*10*4);for(let y=2;y<6;y++)for(const x of [2,3,4,12,13,14])data.set([139,92,246,255],(y*20+x)*4);const c=pixelCandidates(data,20,10,{color:'#8b5cf6',tolerance:3,minArea:4,maxArea:100,roi:{x:100,y:200,width:40,height:20}});assert.equal(c.length,2);assert.ok(c[0].x<120&&c[1].x>120);});

test('entry requires an identifiable single section and bounded waits',()=>{
 const entrance={enabled:true,blockedSelector:'#human',readySelector:'#regions',mapReadySelector:'#map[data-section="A"]',region:{selector:'#A',relative:{x:.5,y:.5}}};
 assert.equal(validateSeatProfile({...p,entrance}).entrance.timeoutMs,180000);
 for(const change of [{blockedSelector:''},{mapReadySelector:''},{region:{x:10,y:20}},{region:{selector:'canvas',kind:'canvas'}},{region:{selector:'#A',relative:{x:2,y:0}}},{mapTimeoutMs:0}])assert.throws(()=>validateSeatProfile({...p,entrance:{...entrance,...change}}));
});
test('live map binding replaces fixed pixel ROI',()=>{
 const pixels={...p,mode:'pixels',color:'#8b5cf6',mapAnchor:{selector:'#map'}};
 assert.equal(validateSeatProfile(pixels).roi,undefined);
 assert.throws(()=>validateSeatProfile({...pixels,mapAnchor:null}));
});
