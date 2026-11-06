import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {it,expect,vi} from 'vitest';
import {DrivesScreen} from '../../src/features/drives/DrivesScreen';
vi.mock('../../src/features/library/SourcePicker',()=>({SourcePicker:()=>null}));
const drive={id:'drive:usb',name:'USB',removable:true,canPowerOff:true,volumes:[{id:'volume:a',driveId:'drive:usb',label:'Movies',mounted:true,mountPoints:['/media/usb'],size:1024,ambiguous:false}]};
const preview={driveIds:['drive:usb','drive:other'],driveNames:['USB','Other disk'],volumeIds:['volume:a','volume:b'],mountedVolumes:2,activeUses:['Video playback','Library scan']};
it('shows shared physical drives before confirmation and never shows safe removal on busy failure',async()=>{
 const api={list:async()=>[drive],mount:async()=>({volumeId:'volume:a'}),prepare:async()=>preview,eject:async()=>{throw new Error('Another application is using this drive.');}};
 render(<DrivesScreen api={api}/>);
 await userEvent.click(await screen.findByRole('button',{name:/Safely remove/}));
 expect(await screen.findByText(/Other disk/)).toBeVisible();
 expect(screen.getByText('Video playback')).toBeVisible();
 expect(screen.getByText('Library scan')).toBeVisible();
 await userEvent.click(screen.getByRole('button',{name:'Stop using and safely remove'}));
 expect(await screen.findByRole('alert')).toHaveTextContent('Another application');
 expect(screen.queryByText(/Safe to remove/)).not.toBeInTheDocument();
});
it('offers mounting for unmounted volumes and reports actual completion',async()=>{
 let mounted=false;
 const api={list:async()=>[{...drive,volumes:[{...drive.volumes[0],mounted}]}],mount:async()=>{mounted=true;return {volumeId:'volume:a'};},prepare:async()=>preview,eject:async()=>({driveIds:preview.driveIds,status:'unmounted' as const})};
 render(<DrivesScreen api={api}/>);
 await userEvent.click(await screen.findByRole('button',{name:'Mount Movies'}));
 expect(await screen.findByText('Volume mounted.')).toBeVisible();
 expect(screen.queryByRole('button',{name:'Mount Movies'})).not.toBeInTheDocument();
});
it('does not offer eject for internal storage',async()=>{
 const api={list:async()=>[{...drive,removable:false}],mount:async()=>({volumeId:'volume:a'}),prepare:async()=>preview,eject:async()=>({driveIds:[],status:'unmounted' as const})};
 render(<DrivesScreen api={api}/>);
 await screen.findByText('Internal storage');
 expect(screen.queryByRole('button',{name:/Safely remove/})).not.toBeInTheDocument();
});
it('reports unmounted completion without claiming the hardware powered off',async()=>{
 const api={list:async()=>[drive],mount:async()=>({volumeId:'volume:a'}),prepare:async()=>preview,eject:async()=>({driveIds:preview.driveIds,status:'unmounted' as const})};
 render(<DrivesScreen api={api}/>);
 await userEvent.click(await screen.findByRole('button',{name:/Safely remove/}));
 await userEvent.click(await screen.findByRole('button',{name:'Stop using and safely remove'}));
 expect(await screen.findByRole('status')).toHaveTextContent('All volumes are unmounted. Safe to disconnect');
 expect(screen.queryByText(/device has been powered off/)).not.toBeInTheDocument();
});
