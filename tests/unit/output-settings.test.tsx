import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import {OutputSettings} from '../../src/features/outputs/OutputSettings';
import {backend} from '../../src/app/backend';
import type {OutputSnapshot} from '../../src/app/contracts';
it('saving a desired target never moves playback and failures remain visible',async()=>{
 const move=vi.fn(async()=>{throw new Error('Audio stream pending');});
 const api={...backend,outputs:async()=>({monitors:[{id:'hdmi',name:'TV',connector:'HDMI-1',primary:false,connected:true,scaleFactor:1,bounds:{x:1920,y:0,width:1920,height:1080}}],audioSinks:[],computerMonitorId:'edp',audioError:null}),selectOutput:vi.fn(async()=>{}),moveSession:move};
 const output:OutputSnapshot={revision:1,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},state:'idle',label:'Computer'};
 render(<OutputSettings output={output} session={{id:'session',kind:'streaming',phase:'ready',target:output.desired,serviceId:'youtube',mediaId:null}} api={api}/>);
 await userEvent.selectOptions(await screen.findByLabelText('Playback'),'hdmi');
 await userEvent.click(screen.getByRole('button',{name:'Save target'}));
 expect(api.selectOutput).toHaveBeenCalledWith({monitorId:'hdmi',audioSinkId:null});expect(move).not.toHaveBeenCalled();
 await userEvent.click(screen.getByText('Connection details'));await userEvent.click(screen.getByRole('button',{name:'Move playback'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Audio stream pending');expect(screen.getByRole('button',{name:'Bring back to computer'})).toBeEnabled();
});
