import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { HomeScreen } from '../../src/features/home/HomeScreen';
import type { HomeSnapshot } from '../../src/features/home/homeModel';
const snapshot: HomeSnapshot = {
  revision: 1, theme: 'dark', vpn: {revision:0,errorCode:null,state:'disconnected',country:null,killSwitch:'unknown',observedAtMs:0},
  outputs: {revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'This computer',state:'ready'},
  summaries: {video:'Add a video folder',audio:'Add a music folder',photo:'Add a photo folder',drives:'No drives connected',resumeProgress:null},
  sessions: [], services: [{id:'youtube',name:'YouTube',logoAsset:'youtube.png',tileColor:'#f90018',vpnRequired:true}],
};
describe('home actions', () => {
  it('opens the selected service by ID without displaying duplicate captions', async () => {
    const action = vi.fn(); render(<HomeScreen snapshot={snapshot} onAction={action}/>);
    const tile = screen.getByRole('button',{name:'YouTube'});
    expect(tile.textContent).toBe('');
    await userEvent.click(tile);
    expect(action).toHaveBeenCalledWith({type:'service',serviceId:'youtube'});
  });
  it('opens the chosen local section and shows the supplied empty state', async () => {
    const action = vi.fn(); render(<HomeScreen snapshot={snapshot} onAction={action}/>);
    await userEvent.click(screen.getByRole('button',{name:/Videos/}));
    expect(action).toHaveBeenCalledWith({type:'navigate',screen:'video'});
    expect(screen.getByText('Add a video folder')).toBeVisible();
    expect(screen.queryByText('United Kingdom')).not.toBeInTheDocument();
    expect(screen.queryByText('LG TV via HDMI')).not.toBeInTheDocument();
  });
  it('does not show a country as connected when the observation is unknown', () => {
    render(<HomeScreen snapshot={{...snapshot,vpn:{...snapshot.vpn,state:'unknown',country:'Finland'}}} onAction={()=>{}}/>);
    expect(screen.queryByText('Finland')).not.toBeInTheDocument();
    expect(screen.getByText('VPN status unknown')).toBeVisible();
  });
});
