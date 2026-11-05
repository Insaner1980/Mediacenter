import {render,screen,act} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import {VpnSettings} from '../../src/features/vpn/VpnSettings';
import {backend} from '../../src/app/backend';
const vpn={revision:1,state:'disconnected',country:null,killSwitch:'disabled',observedAtMs:0,errorCode:null};
it('keeps connection actions disabled until an operation terminates and exposes failure',async()=>{
  let reject!:(error:Error)=>void;
  const api={...backend,countries:async()=>[{id:'Finland',name:'Finland'}],connect:vi.fn(()=>new Promise<void>((_,r)=>{reject=r;}))};
  render(<VpnSettings vpn={vpn} services={[]} api={api}/>);
  await userEvent.selectOptions(await screen.findByLabelText('VPN country'),'Finland');
  await userEvent.click(screen.getByRole('button',{name:'Connect',exact:true}));
  expect(screen.getByRole('button',{name:'Connect',exact:true})).toBeDisabled();
  expect(screen.getByRole('button',{name:'Disconnect',exact:true})).toBeDisabled();
  await act(async()=>reject(new Error('Provider unavailable')));
  expect(await screen.findByRole('alert')).toHaveTextContent('Provider unavailable');
  expect(screen.getByRole('button',{name:'Connect',exact:true})).toBeEnabled();
  expect(api.connect).toHaveBeenCalledWith('Finland');
});
