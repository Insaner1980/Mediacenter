import {it,expect} from 'vitest';
import {RevisionStore,Operations} from '../../src/app/controller';
it('late_event_does_not_overwrite_new_snapshot',()=>{
  const store=new RevisionStore<{revision:number;label:string}>();
  store.accept({revision:8,label:'current'});store.accept({revision:7,label:'old'});
  expect(store.value?.label).toBe('current');
  expect(store.accept({revision:10,label:'gap'})).toBe('gap');
  expect(store.value?.label).toBe('current');
});
it('failed_operation_releases_busy_state',async()=>{
  const operations=new Operations();let reject!:(e:Error)=>void;
  const work=operations.run('connect',()=>new Promise((_,r)=>{reject=r;}));
  expect(operations.busy.has('connect')).toBe(true);
  reject(new Error('failed'));await expect(work).rejects.toThrow('failed');
  expect(operations.busy.has('connect')).toBe(false);
});
it('duplicate_operation_is_not_started',async()=>{
  const operations=new Operations();let resolve!:()=>void;
  const first=operations.run('open',()=>new Promise<void>(r=>{resolve=r;}));
  await expect(operations.run('open',async()=>42)).rejects.toThrow('already running');
  resolve();await first;
});
