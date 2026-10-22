import test from 'node:test';
import assert from 'node:assert/strict';
import {allowedUrl, samePage, parseKst, validateConfig, clockSample, nextPhase} from '../extension/core.js';
test('only exact HTTPS approved domains; no credentials or alternate port', () => {
  assert.equal(allowedUrl('https://nol.yanolja.com/ticket/products/123'),true);
  for (const u of ['http://nol.yanolja.com','https://nol.yanolja.com.evil.test','https://u:p@nol.yanolja.com','https://nol.yanolja.com:123','javascript:alert(1)']) assert.equal(allowedUrl(u),false);
});
test('page identity ignores query but not host or path', () => {
  assert.ok(samePage('https://ticket.interpark.com/Seat?a=1','https://ticket.interpark.com/Seat?b=2'));
  assert.equal(samePage('https://ticket.interpark.com/Seat','https://ticket.interpark.com/SeatWrong'),false);
});
test('KST parsed independently of computer timezone with invalid dates rejected', () => {
  assert.equal(parseKst('2026-10-18T20:00:01'),Date.parse('2026-10-18T11:00:01Z'));
  assert.equal(parseKst(''),null);
  for(const date of ['2026-02-30T20:00','2026-01-01T24:01','bad']) assert.throws(()=>parseKst(date));
});
test('auto macro cannot arm without a ready-page condition', () => {
  const c={url:'https://nol.yanolja.com/ticket/products/26010352',buttonText:'예매하기',timeoutMinutes:30,saleAt:'',autoMacro:true};
  assert.throws(()=>validateConfig(c));
  assert.equal(validateConfig({...c,readyUrl:'https://ticket.interpark.com/Seat',readySelector:'#seat'}).autoMacro,true);
});
test('HTTP Date includes quantization uncertainty, rejects cache and long RTT', () => {
  const epoch=Date.parse('2026-10-18T10:00:00Z');
  const c=clockSample('Sun, 18 Oct 2026 10:00:00 GMT','0',epoch,epoch+100);
  assert.equal(c.offsetMs,450); assert.equal(c.uncertaintyMs,550);
  assert.throws(()=>clockSample('bad',null,1,2));
  assert.throws(()=>clockSample(new Date(epoch).toUTCString(),'2',epoch,epoch+100));
  assert.throws(()=>clockSample(new Date(epoch).toUTCString(),null,epoch,epoch+4000));
});
test('one-shot state machine rejects repeats and out-of-order events', () => {
  assert.equal(nextPhase('ARMED','CLICK'),'WAIT_READY');
  assert.equal(nextPhase('WAIT_READY','START'),'STARTING');
  assert.equal(nextPhase('STARTING','DONE'),'DONE');
  assert.throws(()=>nextPhase('WAIT_READY','CLICK'));
  assert.throws(()=>nextPhase('DONE','START'));
});
