import type { ReactNode } from 'react';
import type { HomeSnapshot } from '../features/home/homeModel';
import { Icon } from './Icons';
import {useVpnObservation} from '../features/vpn/useVpnObservation';
export function AppShell({snapshot, children}: {snapshot: HomeSnapshot; children: ReactNode}) {
  const vpn = useVpnObservation(snapshot.vpn);
  const connected = vpn.state === 'connected';
  const vpnLabel: Record<string,string> = {connected:'NordVPN connected',disconnected:'VPN disconnected',unknown:'VPN status unknown',unavailable:'NordVPN unavailable',connecting:'VPN connecting',disconnecting:'VPN disconnecting',unauthenticated:'Sign in to NordVPN',error:'VPN status unavailable'};
  return <div className="app-container"><main className="app">
    <header className="app-header">
      <div className="brand"><span className="brand-mark" aria-hidden="true"><svg viewBox="0 0 16 16"><path d="M4 2 14 8 4 14z"/></svg></span>mediacenter</div>
      <div className="connections">
        <div className="connection connection-vpn" data-connected={connected}>
          <Icon name="shield"/>
          <div><span className="connection-label">{vpnLabel[vpn.state] ?? 'VPN status unknown'}</span><span className="connection-value">{connected ? vpn.country ?? 'Country unavailable' : 'Connection settings'}</span></div>
        </div>
        <div className="connection"><Icon name="display"/><div><span className="connection-label">{snapshot.outputs.state === 'ready' ? 'Picture & sound' : 'Playback output'}</span><span className="connection-value">{snapshot.outputs.label}</span></div></div>
      </div>
    </header>
    {children}
    <footer className="app-footer"><div className="output-path"><Icon name="laptop"/><span><strong>Controls on this computer</strong></span></div><span>Playback output: {snapshot.outputs.label}</span></footer>
  </main></div>;
}
