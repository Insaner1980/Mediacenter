import test from 'node:test';
import assert from 'node:assert/strict';
import { TargetRegistry } from '../target-registry.mjs';
test('target generation prevents input after navigation, detach, replacement and restart', () => {
 for (const reason of ['navigation','detach','replacement','restart','unenroll']) {
 const r=new TargetRegistry(); r.enroll('p'); r.issue('p','n','http://127.0.0.1:41739',100);
 r.launch('p','n',1); const target=r.attach(9,'s','g');
 assert.equal(r.validate('s','g',9).tabId,9);
 assert.throws(()=>r.validate('s','g',10));
 r.invalidate(reason); assert.throws(()=>r.validate('s','g',9));
 assert.equal(target.tabId,9);
 }
});
