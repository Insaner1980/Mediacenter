import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {ExitDialog} from '../../src/features/settings/ExitDialog';
import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
import type {ExitStatus} from '../../src/app/contracts';
const state=vi.hoisted(()=>({emit:((()=>{}) as (s:ExitStatus)=>void)}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:vi.fn(async()=>{})}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name:string,callback:(event:{payload:ExitStatus})=>void)=>{if(name==='exit_requested')state.emit=value=>callback({payload:value});return()=>{};})}));
it.each([['Keep open','keep_open',true],['Stop playback and quit','stop_playback_and_quit',true],['Leave service open and quit','leave_service_open_and_quit',false],['Close service and quit','close_service_and_quit',false]] as const)('retains native policy for %s',async(label,action,local)=>{render(<><button>Opener</button><ExitDialog/></>);await act(async()=>state.emit({localActive:local,backgroundActive:false,streamingActive:true,remoteEnabled:false,explicitQuit:false}));expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);await userEvent.click(screen.getByRole('button',{name:label}));await waitFor(()=>expect(invoke).toHaveBeenCalledWith('request_exit',{action}));});
it('releases the exit request listener when the error listener fails to subscribe',async()=>{
 const stop=vi.fn();
 vi.mocked(listen).mockResolvedValueOnce(stop).mockRejectedValueOnce(new Error('Event unavailable'));
 const view=render(<ExitDialog/>);
 await act(async()=>{});
 view.unmount();
 expect(stop).toHaveBeenCalledTimes(1);
});
