import {useEffect,useState} from 'react';
import {useVpnObservation} from './useVpnObservation';
import {backend,errorMessage} from '../../app/backend';
import type {VpnSnapshot,ServiceDefinition,CountryOption} from '../../app/contracts';
export function VpnSettings({vpn:observation,services,api=backend}:{vpn:VpnSnapshot;services:ServiceDefinition[];api?:typeof backend}) {
  const vpn=useVpnObservation(observation);
  const [countries,setCountries]=useState<CountryOption[]>([]);const [country,setCountry]=useState('');
  const [busy,setBusy]=useState(false);const [error,setError]=useState('');
  useEffect(()=>{let live=true;void api.countries().then(value=>{if(live)setCountries(value);}).catch(reason=>{if(live)setError(errorMessage(reason));});return()=>{live=false;};},[api]);
  async function run(work:()=>Promise<unknown>) {if(busy)return;setBusy(true);setError('');try{await work();}catch(reason){setError(errorMessage(reason));}finally{setBusy(false);}}
  return <section><h2>NordVPN</h2><p>Connection: {vpn.state}{vpn.country && vpn.state==='connected' ? ` (${vpn.country})` : ''}</p><p>Kill Switch: {vpn.killSwitch}</p>
    <label className="field">VPN country<select value={country} onChange={event=>setCountry(event.target.value)} disabled={busy}><option value="">Provider recommended country</option>{countries.map(option=><option key={option.id} value={option.id}>{option.name}</option>)}</select></label>
    <p className="muted">Connect and Disconnect affect this computer's network connection, including other applications.</p>
    <div className="actions"><button className="control" disabled={busy} onClick={()=>void run(()=>api.connect(country||null))}>Connect</button><button className="control" disabled={busy} onClick={()=>void run(api.disconnect)}>Disconnect</button><button className="control" disabled={busy} onClick={()=>void run(api.refreshVpn)}>Refresh status</button></div>
    {vpn.killSwitch!=='enabled' && <><p className="muted">Kill Switch blocks this computer's network traffic when the VPN connection is lost.</p><button className="control" disabled={busy} onClick={()=>void run(api.enableKillSwitch)}>Enable NordVPN Kill Switch</button></>}
    {busy && <p role="status">Waiting for NordVPN</p>}{error && <p role="alert" className="message">{error}</p>}
    <h3>Service connection requirements</h3><p className="muted">Turning off a requirement allows that service to use the current connection. It does not disconnect NordVPN.</p>
    {services.map(service=><label className="field" key={service.id}><span><input type="checkbox" checked={service.vpnRequired} disabled={busy} onChange={event=>void run(()=>api.setVpnRequirement(service.id,event.target.checked))}/> Require VPN for {service.name}</span></label>)}
  </section>;
}
