import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {RemoteSettings} from '../../src/features/remote/RemoteSettings';
import type {RemoteDevice,RemoteEnableRequest,RemoteSettingsSnapshot} from '../../src/app/contracts';
const initial:RemoteSettingsSnapshot={revision:1,enabled:false,listening:false,advertising:false,interfaceName:null,port:47832,interfaces:[{name:'wlan0',label:'Wi-Fi (wlan0)',addresses:['192.168.1.4']}],addresses:[],requiresRepairing:false,status:'Remote control is off.',pairingAttemptId:null};
it('requires a selected network and explicit enablement; persists an explicit alternative port',async()=>{
 const api={settings:vi.fn(async()=>initial),setEnabled:vi.fn(async(config:RemoteEnableRequest)=>({...initial,revision:2,enabled:config.enabled,listening:config.enabled,interfaceName:config.interfaceName,port:config.port,status:'Listening'})),devices:vi.fn(async()=>[]),beginPairing:vi.fn(),cancelPairing:vi.fn(),revokeDevice:vi.fn()};
 const user=userEvent.setup();render(<RemoteSettings api={api}/>);
 await screen.findByText('Remote control is off.');expect(api.setEnabled).not.toHaveBeenCalled();expect(screen.getByRole('button',{name:'Enable remote control'})).toBeDisabled();
 await user.selectOptions(screen.getByLabelText('Local network'),'wlan0');await user.clear(screen.getByLabelText('Port'));await user.type(screen.getByLabelText('Port'),'47840');await user.click(screen.getByRole('button',{name:'Enable remote control'}));
 await waitFor(()=>expect(screen.getByRole('button',{name:'Disable remote control'})).toBeEnabled());expect(api.setEnabled).toHaveBeenCalledWith({revision:1,enabled:true,interfaceName:'wlan0',port:47840});expect(screen.queryByText(/TV mode|monitor selection/i)).not.toBeInTheDocument();
 await user.click(screen.getByRole('button',{name:'Disable remote control'}));await screen.findByRole('button',{name:'Enable remote control'});
});
it('shows actionable native errors and requires new pairing after identity recovery',async()=>{
 const api={settings:vi.fn(async()=>({...initial,requiresRepairing:true,status:'Choose an available port.'})),setEnabled:vi.fn(async()=>{throw {message:'Port occupied. Choose another port.'};}),devices:vi.fn(async()=>[]),beginPairing:vi.fn(),cancelPairing:vi.fn(),revokeDevice:vi.fn()};
 const user=userEvent.setup();render(<RemoteSettings api={api}/>);await screen.findByText(/Pair your phones again/);await user.selectOptions(screen.getByLabelText('Local network'),'wlan0');await user.click(screen.getByRole('button',{name:'Enable remote control'}));expect(await screen.findByRole('alert')).toHaveTextContent('Port occupied');
});

const listening:RemoteSettingsSnapshot={...initial,enabled:true,listening:true,interfaceName:'wlan0',status:'Listening'};
const display={attemptId:'attempt-old',qrRows:['111','101','111'],addresses:['192.168.1.4'],port:47832,manualCode:'ABCDEFGH2345',verificationWords:['able','about','absent','acid','across','act'],expiresInMs:120000};
function deferred<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(done=>{resolve=done;});return {promise,resolve};}
function fakeApi(){return {settings:vi.fn(async()=>listening),setEnabled:vi.fn(async()=>initial),devices:vi.fn(async()=>[]),beginPairing:vi.fn(async()=>display),cancelPairing:vi.fn(async()=>{}),revokeDevice:vi.fn(async()=>{})};}
it('shows remote settings when the paired-phone list cannot be fetched',async()=>{
 const api=fakeApi();api.devices.mockRejectedValueOnce(new Error('Paired phone list unavailable'));
 render(<RemoteSettings api={api}/>);
 expect(await screen.findByRole('alert')).toHaveTextContent('Paired phone list unavailable');
 expect(screen.getByRole('button',{name:'Pair a phone'})).toBeEnabled();
});
it('keeps an active pairing available to cancel again after a failed cancellation',async()=>{
 const api=fakeApi();api.cancelPairing.mockRejectedValueOnce(new Error('Could not cancel pairing'));
 const user=userEvent.setup();render(<RemoteSettings api={api}/>);
 await user.click(await screen.findByRole('button',{name:'Pair a phone'}));
 await user.click(await screen.findByRole('button',{name:'Cancel pairing'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Could not cancel pairing');
 await user.click(screen.getByRole('button',{name:'Cancel pairing'}));
 await waitFor(()=>expect(screen.queryByRole('img',{name:'Pairing QR code'})).not.toBeInTheDocument());
 expect(api.cancelPairing).toHaveBeenNthCalledWith(2,display.attemptId);
});
it('cancels the exact pending attempt after the API owner changes without exposing its secrets',async()=>{
 const pending=deferred<typeof display>(),oldApi=fakeApi(),newApi=fakeApi();oldApi.beginPairing.mockImplementation(()=>pending.promise);
 const user=userEvent.setup();const view=render(<RemoteSettings api={oldApi}/>);await user.click(await screen.findByRole('button',{name:'Pair a phone'}));
 view.rerender(<RemoteSettings api={newApi}/>);pending.resolve(display);
 await waitFor(()=>expect(oldApi.cancelPairing).toHaveBeenCalledWith('attempt-old'));
 expect(newApi.cancelPairing).not.toHaveBeenCalled();expect(screen.queryByText('ABCDEFGH2345')).not.toBeInTheDocument();
});
it('ignores a settings response issued before the current pairing attempt',async()=>{
 const pending=deferred<RemoteSettingsSnapshot>(),api=fakeApi();let calls=0;
 api.settings.mockImplementation(()=>++calls===1?Promise.resolve(listening):pending.promise);
 const user=userEvent.setup();render(<RemoteSettings api={api}/>);await screen.findByRole('button',{name:'Pair a phone'});
 await waitFor(()=>expect(api.settings).toHaveBeenCalledTimes(2),{timeout:2500});
 await user.click(screen.getByRole('button',{name:'Pair a phone'}));await screen.findByRole('img',{name:'Pairing QR code'});
 await act(async()=>pending.resolve(listening));
 expect(screen.getByRole('img',{name:'Pairing QR code'})).toBeInTheDocument();
});
it('erases pairing secrets immediately when disabling even if the command fails',async()=>{
 const api=fakeApi();api.setEnabled.mockRejectedValue(new Error('Could not save settings'));
 const user=userEvent.setup();render(<RemoteSettings api={api}/>);await user.click(await screen.findByRole('button',{name:'Pair a phone'}));await screen.findByRole('img');
 await user.click(screen.getByRole('button',{name:'Disable remote control'}));
 expect(screen.queryByRole('img')).not.toBeInTheDocument();expect(screen.queryByText('ABCDEFGH2345')).not.toBeInTheDocument();
 expect(api.cancelPairing).toHaveBeenCalledWith('attempt-old');
});

it('cancels a pairing result that arrives after unmount and preserves the other owner',async()=>{
 const pending=deferred<typeof display>(),api=fakeApi();api.beginPairing.mockImplementation(()=>pending.promise);
 const user=userEvent.setup();const view=render(<RemoteSettings api={api}/>);await user.click(await screen.findByRole('button',{name:'Pair a phone'}));view.unmount();
 await act(async()=>pending.resolve(display));expect(api.cancelPairing).toHaveBeenCalledWith('attempt-old');
 expect(screen.queryByText('ABCDEFGH2345')).not.toBeInTheDocument();
});
it('revokes only after confirmation and refreshes the metadata list',async()=>{
 const api=fakeApi();let phones:RemoteDevice[]=[{deviceId:'phone',label:'My phone',createdAtMs:1,lastSeenAtMs:null}];
 const devices=vi.fn(async()=>phones),revokeDevice=vi.fn(async(id:string)=>{phones=phones.filter(phone=>phone.deviceId!==id);});
 const user=userEvent.setup();render(<RemoteSettings api={{...api,devices,revokeDevice}}/>);
 await user.click(await screen.findByRole('button',{name:'Revoke My phone'}));expect(revokeDevice).not.toHaveBeenCalled();
 await user.click(screen.getByRole('button',{name:'Keep phone'}));expect(screen.queryByRole('group',{name:'Confirm revocation'})).not.toBeInTheDocument();
 await user.click(screen.getByRole('button',{name:'Revoke My phone'}));await user.click(screen.getByRole('button',{name:'Confirm revocation'}));
 await screen.findByText('No paired phones.');expect(revokeDevice).toHaveBeenCalledWith('phone');
});
it('removes an active ceremony when the setup API owner changes',async()=>{
 const oldApi=fakeApi(),newApi=fakeApi(),user=userEvent.setup();
 const view=render(<RemoteSettings api={oldApi}/>);await user.click(await screen.findByRole('button',{name:'Pair a phone'}));await screen.findByRole('img');
 view.rerender(<RemoteSettings api={newApi}/>);
 await waitFor(()=>expect(screen.queryByRole('img')).not.toBeInTheDocument());
 expect(oldApi.cancelPairing).toHaveBeenCalledWith('attempt-old');expect(newApi.cancelPairing).not.toHaveBeenCalled();
});
