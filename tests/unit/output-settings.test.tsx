import {render,screen,waitFor,within} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import {OutputSettings} from '../../src/features/outputs/OutputSettings';
import {backend} from '../../src/app/backend';
import type {OutputList,OutputSnapshot} from '../../src/app/contracts';
const output:OutputSnapshot={revision:1,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},state:'idle',label:'Computer'};
const monitor=(id:string,name:string)=>({id,name,connector:id,primary:false,connected:true,scaleFactor:1,bounds:{x:0,y:0,width:1920,height:1080}});
const outputs=(computerMonitorId:string):OutputList=>({monitors:[monitor('edp','Desk display'),monitor('hdmi','TV')],audioSinks:[],computerMonitorId,audioError:null});
it('saving a desired target never moves playback and failures remain visible',async()=>{
 const move=vi.fn(async()=>{throw new Error('Audio stream pending');});
 const api={...backend,outputs:async()=>({monitors:[{id:'hdmi',name:'TV',connector:'HDMI-1',primary:false,connected:true,scaleFactor:1,bounds:{x:1920,y:0,width:1920,height:1080}}],audioSinks:[],computerMonitorId:'edp',audioError:null}),selectOutput:vi.fn(async()=>{}),moveSession:move};
 render(<OutputSettings output={output} session={{id:'session',kind:'streaming',phase:'ready',target:output.desired,serviceId:'youtube',mediaId:null}} api={api}/>);
 await userEvent.selectOptions(await screen.findByLabelText('Playback'),'hdmi');
 await userEvent.click(screen.getByRole('button',{name:'Save target'}));
 expect(api.selectOutput).toHaveBeenCalledWith({monitorId:'hdmi',audioSinkId:null});expect(move).not.toHaveBeenCalled();
 await userEvent.click(screen.getByText('Connection details'));await userEvent.click(screen.getByRole('button',{name:'Move playback'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Audio stream pending');expect(screen.getByRole('button',{name:'Bring back to computer'})).toBeEnabled();
});
it('reports a saved target when moving an existing session fails',async()=>{
 const api={...backend,outputs:async()=>outputs('edp'),selectOutput:vi.fn(async()=>{}),moveSession:vi.fn(async()=>{throw new Error('Audio stream pending');})};
 render(<OutputSettings compact output={output} session={{id:'session',kind:'streaming',phase:'ready',target:output.desired,serviceId:'youtube',mediaId:null}} api={api}/>);
 await waitFor(()=>expect(within(screen.getByLabelText('Picture')).getByRole('option',{name:'TV'})).toBeVisible());
 await userEvent.selectOptions(screen.getByLabelText('Picture'),'hdmi');
 await userEvent.click(screen.getByRole('button',{name:'Apply to playback'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Playback target saved.');
 expect(screen.getByRole('alert')).toHaveTextContent('Audio stream pending');
});
it('refreshes the selected control display with the output list',async()=>{
 const api={...backend,outputs:vi.fn(async()=>outputs('hdmi')).mockResolvedValueOnce(outputs('edp'))};
 render(<OutputSettings output={output} api={api}/>);
 const controls=screen.getByLabelText('Controls');
 await waitFor(()=>expect(controls).toHaveValue('edp'));
 await userEvent.click(screen.getByText('Connection details'));
 await userEvent.click(screen.getByRole('button',{name:'Refresh outputs'}));
 await waitFor(()=>expect(controls).toHaveValue('hdmi'));
});
