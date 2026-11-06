import {render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {OutputSettings} from '../../src/features/outputs/OutputSettings';
import {VpnSettings} from '../../src/features/vpn/VpnSettings';
import {productionServices} from '../../src/app/AppServices';
import {referenceSnapshot} from '../../src/testing/fixtures';
it('saves the control monitor immediately and preserves a dirty target through snapshot changes',async()=>{
 const monitor=(id:string)=>({id,name:id,connected:true,connector:id,primary:false,scaleFactor:1,bounds:{x:0,y:0,width:1920,height:1080}});const api={...productionServices.backend,outputs:async()=>({monitors:[monitor('edp'),monitor('hdmi')],audioSinks:[],computerMonitorId:'edp',audioError:null}),setControlMonitor:vi.fn(async()=>{}),selectOutput:vi.fn(async()=>{})};const user=userEvent.setup();const view=render(<OutputSettings output={referenceSnapshot.outputs} api={api}/>);
 await user.selectOptions(await screen.findByLabelText('Controls'),'hdmi');expect(api.setControlMonitor).toHaveBeenCalledWith('hdmi');await user.selectOptions(screen.getByLabelText('Playback'),'hdmi');view.rerender(<OutputSettings output={{...referenceSnapshot.outputs,revision:99}} api={api}/>);expect(screen.getByLabelText('Playback')).toHaveValue('hdmi');
});
it('keeps a connected country change as a draft until Apply country',async()=>{
 const api={...productionServices.backend,countries:async()=>[{id:'fi',name:'Finland'},{id:'se',name:'Sweden'}],connect:vi.fn(async()=>{})};const user=userEvent.setup();render(<VpnSettings vpn={{...referenceSnapshot.vpn,country:'Finland',observedAtMs:Date.now()}} services={[]} api={api}/>);await waitFor(()=>expect(screen.getByLabelText('VPN country')).toHaveValue('fi'));await user.selectOptions(screen.getByLabelText('VPN country'),'se');expect(api.connect).not.toHaveBeenCalled();await user.click(screen.getByRole('button',{name:'Apply country'}));expect(api.connect).toHaveBeenCalledWith('se');
});
