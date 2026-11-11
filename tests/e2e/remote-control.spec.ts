import {test,expect,type Page} from '@playwright/test';
import {referenceSnapshot} from '../../src/testing/fixtures';
import {outputs} from '../../src/testing/redesignFixtures';

type Call={command:string;args:Record<string,unknown>};
declare global {interface Window {task21:{calls:Call[];unexpected:string[];emit:(event:string,payload:unknown)=>void}}}

// Test-only transport installed before application modules load. Every effect is local data.
async function setup(page:Page){
 await page.addInitScript(({home,outputList})=>{
  const callbacks=new Map<number,(data:unknown)=>void>(),listeners=new Map<string,number[]>();let next=0;
  const host=window as unknown as Record<string,unknown>;
  const snapshot={revision:1,enabled:false,listening:false,advertising:false,interfaceName:null as string|null,port:47832,interfaces:[{name:'wlan0',label:'Fixture Wi-Fi',addresses:['192.168.1.4']}],addresses:[] as string[],requiresRepairing:false,status:'Remote control is off.',pairingAttemptId:null as string|null};
  let devices=[{deviceId:'fixture-phone',label:'Fixture phone',createdAtMs:1,lastSeenAtMs:null}];
  const calls:Call[]=[],unexpected:string[]=[];
  host.isTauri=true;
  window.task21={calls,unexpected,emit:(event,payload)=>{for(const id of listeners.get(event)??[])callbacks.get(id)?.({event,id,payload});}};
  host.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'},currentWebview:{label:'main'}},transformCallback:(callback:(data:unknown)=>void)=>{callbacks.set(++next,callback);return next;},unregisterCallback:(id:number)=>callbacks.delete(id),invoke:async(command:string,args:Record<string,unknown>={})=>{
   if(command==='plugin:event|listen'){const event=String(args.event),id=Number(args.handler);listeners.set(event,[...(listeners.get(event)??[]),id]);return id;}
   if(command==='plugin:event|unlisten'){listeners.set(String(args.event),(listeners.get(String(args.event))??[]).filter(id=>id!==args.eventId));return;}
   calls.push({command,args});
   switch(command){
    case 'get_home_snapshot':return home;
    case 'get_player':case 'get_slideshow':return null;
    case 'list_media_sources':case 'list_playlists':case 'list_vpn_countries':return [];
    case 'ensure_default_media_source':return null;
    case 'query_media':return {items:[],nextCursor:null};
    case 'get_music_queue':return {revision:1,items:[],originalMediaIds:[],currentIndex:null,sessionId:null,shuffle:false,repeat:'off',message:null};
    case 'list_outputs':return outputList;
    case 'get_remote_settings':return structuredClone(snapshot);
    case 'list_remote_devices':return structuredClone(devices);
    case 'set_remote_enabled':{const config=args.config as {enabled:boolean;interfaceName:string;port:number};if(config.port===47833)throw {message:'Port occupied. Choose another port.'};Object.assign(snapshot,config,{revision:snapshot.revision+1,listening:config.enabled,status:config.enabled?'Listening on fixture Wi-Fi.':'Remote control is off.'});return structuredClone(snapshot);}
    case 'begin_remote_pairing':snapshot.pairingAttemptId='fixture-attempt';return {attemptId:'fixture-attempt',qrRows:Array(21).fill('0'.repeat(21)),addresses:['192.168.1.4'],port:snapshot.port,manualCode:'ABCDEFGH2345',verificationWords:['able','about','absent','acid','across','act'],expiresInMs:120000};
    case 'cancel_remote_pairing':if(args.attemptId===snapshot.pairingAttemptId)snapshot.pairingAttemptId=null;return;
    case 'revoke_remote_device':devices=devices.filter(device=>device.deviceId!==args.deviceId);return;
    case 'register_remote_renderer':return '1';
    case 'get_renderer_confirmations':return [];
    case 'update_remote_renderer':case 'acknowledge_remote_renderer':case 'request_exit':return;
    default:unexpected.push(command);throw Error(`Unexpected Task21 IPC: ${command}`);
   }
  }};
  host.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:(_event:string,id:number)=>callbacks.delete(id)};
 },{home:referenceSnapshot,outputList:outputs});
 await page.goto('/');
 await page.getByRole('button',{name:'Settings',exact:true}).click();
 await expect(page.getByRole('heading',{name:'Settings',exact:true})).toBeVisible();
}
async function calls(page:Page,command:string){return page.evaluate(name=>window.task21.calls.filter(call=>call.command===name),command);}

test('remote setup requires trusted network, recovers occupied port and clears pairing on close',async({page})=>{
 await setup(page);await page.getByRole('button',{name:'Remote control',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Remote control',exact:true});
 await expect(dialog.getByRole('button',{name:'Enable remote control'})).toBeDisabled();
 expect(await calls(page,'set_remote_enabled')).toHaveLength(0);
 await dialog.getByLabel('Local network').selectOption('wlan0');await dialog.getByLabel('Port',{exact:true}).fill('47833');await dialog.getByRole('button',{name:'Enable remote control'}).click();
 await expect(dialog.getByRole('alert')).toHaveText('Port occupied. Choose another port.');
 await dialog.getByLabel('Port',{exact:true}).fill('47840');await dialog.getByRole('button',{name:'Enable remote control'}).click();
 await expect(dialog.getByRole('button',{name:'Disable remote control'})).toBeEnabled();
 expect((await calls(page,'set_remote_enabled')).at(-1)?.args).toEqual({config:{revision:1,enabled:true,interfaceName:'wlan0',port:47840}});
 await dialog.getByRole('button',{name:'Pair a phone'}).click();await expect(dialog.getByRole('img',{name:'Pairing QR code'})).toBeVisible();
 await page.keyboard.press('Escape');await expect(dialog).toHaveCount(0);
 await expect.poll(()=>calls(page,'cancel_remote_pairing')).toEqual([{command:'cancel_remote_pairing',args:{attemptId:'fixture-attempt'}}]);
 await page.getByRole('button',{name:'Remote control',exact:true}).click();await expect(dialog.getByRole('img')).toHaveCount(0);
 await dialog.getByRole('button',{name:'Disable remote control'}).click();await expect(dialog.getByRole('button',{name:'Enable remote control'})).toBeEnabled();
 expect(await page.evaluate(()=>window.task21.unexpected)).toEqual([]);
});

test('revocation requires confirmation and library browsing stays free of playback bars',async({page})=>{
 await setup(page);await page.getByRole('button',{name:'Remote control',exact:true}).click();
 const dialog=page.getByRole('dialog',{name:'Remote control',exact:true});
 await dialog.getByRole('button',{name:'Revoke Fixture phone'}).click();await dialog.getByRole('button',{name:'Keep phone'}).click();expect(await calls(page,'revoke_remote_device')).toHaveLength(0);
 await dialog.getByRole('button',{name:'Revoke Fixture phone'}).click();await dialog.getByRole('button',{name:'Confirm revocation'}).click();await expect(dialog.getByText('No paired phones.')).toBeVisible();
 expect(await calls(page,'revoke_remote_device')).toEqual([{command:'revoke_remote_device',args:{deviceId:'fixture-phone'}}]);
 await dialog.getByRole('button',{name:'Close',exact:true}).click();await expect(dialog).toHaveCount(0);
 for(const name of ['Videos','Music','Photos']){await page.getByRole('button',{name,exact:true}).click();await expect(page.getByRole('region',{name:'Playback',exact:true})).toHaveCount(0);}
 expect(await page.evaluate(()=>window.task21.unexpected)).toEqual([]);
});

test('close offers remote availability while explicit quit explains phone reconnect limitation',async({page})=>{
 await setup(page);
 await expect.poll(()=>page.evaluate(()=>window.task21.calls.some(call=>call.command==='register_remote_renderer'))).toBe(true);
 const status={localActive:false,streamingActive:false,backgroundActive:false,remoteEnabled:true,explicitQuit:false};
 await page.evaluate(value=>window.task21.emit('exit_requested',value),status);
 const dialog=page.getByRole('dialog',{name:'Close Mediacenter?',exact:true});
 await expect(dialog).toContainText('Phone reconnection requires relaunching Mediacenter on the computer.');
 await dialog.getByRole('button',{name:'Keep available for Remote'}).click();await expect(dialog).toHaveCount(0);
 expect(await calls(page,'request_exit')).toEqual([{command:'request_exit',args:{action:'keep_available'}}]);
 await page.evaluate(value=>window.task21.emit('exit_requested',{...value,explicitQuit:true}),status);
 const quit=page.getByRole('dialog',{name:'Quit Mediacenter?',exact:true});await expect(quit.getByRole('button',{name:'Keep available for Remote'})).toHaveCount(0);
 await quit.getByRole('button',{name:'Cancel',exact:true}).click();await expect(quit).toHaveCount(0);
 expect((await calls(page,'request_exit')).at(-1)?.args).toEqual({action:'keep_open'});
 expect(await page.evaluate(()=>window.task21.unexpected)).toEqual([]);
});
