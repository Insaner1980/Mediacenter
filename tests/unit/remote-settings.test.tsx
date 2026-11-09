import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {RemoteSettings} from '../../src/features/remote/RemoteSettings';
import type {RemoteSettingsSnapshot} from '../../src/app/contracts';
const initial:RemoteSettingsSnapshot={revision:1,enabled:false,listening:false,advertising:false,interfaceName:null,port:47832,interfaces:[{name:'wlan0',label:'Wi-Fi (wlan0)',addresses:['192.168.1.4']}],addresses:[],requiresRepairing:false,status:'Remote control is off.',pairingAttemptId:null};
it('requires a selected network and explicit enablement; persists an explicit alternative port',async()=>{
 const api={settings:vi.fn(async()=>initial),setEnabled:vi.fn(async(config)=>({...initial,revision:2,enabled:config.enabled,listening:config.enabled,interfaceName:config.interfaceName,port:config.port,status:'Listening'})),devices:vi.fn(async()=>[]),beginPairing:vi.fn(),cancelPairing:vi.fn(),revokeDevice:vi.fn()};
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
