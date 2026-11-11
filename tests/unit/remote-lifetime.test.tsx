import {act,render,screen,waitFor} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {ExitDialog} from '../../src/features/settings/ExitDialog';
import {invoke} from '@tauri-apps/api/core';
const state=vi.hoisted(()=>({emit:((()=>{}) as (value:unknown)=>void)}));
vi.mock('@tauri-apps/api/core',()=>({isTauri:()=>true,invoke:vi.fn(async()=>{})}));
vi.mock('@tauri-apps/api/event',()=>({listen:vi.fn(async(name:string,callback:(event:{payload:unknown})=>void)=>{if(name==='exit_requested')state.emit=value=>callback({payload:value});return()=>{};})}));
it('offers remembered background availability and explains explicit quit',async()=>{
 render(<ExitDialog/>);
 await act(async()=>state.emit({localActive:true,streamingActive:false,backgroundActive:false,remoteEnabled:true}));
 expect(screen.getByText(/phone.*relaunch/i)).toBeVisible();
 await userEvent.click(screen.getByRole('button',{name:'Keep available for Remote'}));
 await waitFor(()=>expect(invoke).toHaveBeenCalledWith('request_exit',{action:'keep_available'}));
 await waitFor(()=>expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
});
