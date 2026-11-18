import {screen,within,waitFor} from '@testing-library/react';
import {expect,it,vi} from 'vitest';
import {renderApp} from './helpers/renderApp';
import {createFixtureServices} from '../../src/testing/fixtureServices';

it('opens and reopens Picture & sound on Home and Settings without navigating',async()=>{
 const services=createFixtureServices({screen:'home',state:'populated',theme:'dark'});
 const {user}=renderApp({services});
 const opener=screen.getByRole('button',{name:'Picture & sound'});
 await user.click(opener);
 let dialog=await screen.findByRole('dialog',{name:'Picture & sound'});
 expect(within(dialog).getByLabelText('Picture')).toBeVisible();
 expect(within(dialog).getByLabelText('Sound')).toBeVisible();
 expect(screen.queryByRole('heading',{name:'Settings'})).not.toBeInTheDocument();
 await user.keyboard('{Escape}');expect(opener).toHaveFocus();
 await user.click(screen.getByRole('button',{name:'Settings'}));
 for(let i=0;i<2;i++){
  await user.click(screen.getByRole('button',{name:'Picture & sound'}));
  dialog=await screen.findByRole('dialog',{name:'Picture & sound'});
  await user.click(within(dialog).getByRole('button',{name:'Close'}));
  expect(screen.getByRole('heading',{name:'Settings'})).toBeVisible();
 }
});
it('applies the selected output only on explicit Apply, including current playback',async()=>{
 const services=createFixtureServices({screen:'audio',state:'populated',theme:'dark'});
 const save=vi.spyOn(services.backend,'selectOutput'),move=vi.spyOn(services.backend,'moveSession');
 const {user}=renderApp({services,initialNavigation:{route:'audio'}});
 await user.click(screen.getByRole('button',{name:'Picture & sound'}));
 const dialog=await screen.findByRole('dialog',{name:'Picture & sound'});
 await waitFor(()=>expect(within(dialog).getByRole('button',{name:'Apply to playback'})).toBeEnabled());
 await user.selectOptions(within(dialog).getByLabelText('Picture'),'');
 await user.selectOptions(within(dialog).getByLabelText('Sound'),'');
 expect(save).not.toHaveBeenCalled();expect(move).not.toHaveBeenCalled();
 await user.click(within(dialog).getByRole('button',{name:'Apply to playback'}));
 await waitFor(()=>expect(move).toHaveBeenCalledWith('audio-session',{monitorId:null,audioSinkId:null}));
 expect(save).toHaveBeenCalledWith({monitorId:null,audioSinkId:null});
});
it('keeps folder management in Settings and never duplicates external music controls',async()=>{
 const services=createFixtureServices({screen:'audio',state:'populated',theme:'light'});
 const {user}=renderApp({services,initialNavigation:{route:'audio'}});
 await screen.findByText('Evening piano.flac');
 expect(screen.queryByRole('region',{name:'Playback'})).not.toBeInTheDocument();
 for(const title of ['Photos','Videos','Music']){
  await user.click(screen.getByRole('button',{name:title}));
  expect(screen.getByRole('button',{name:'Open folder'})).toBeVisible();
  expect(screen.queryByRole('button',{name:'Manage folders'})).not.toBeInTheDocument();
 }
 await user.click(screen.getByRole('button',{name:'Settings'}));
 await user.click(screen.getByRole('button',{name:'Media folders'}));
 expect(await screen.findByRole('dialog',{name:'Media folders'})).toBeVisible();
 expect(screen.getByRole('button',{name:'Add folder'})).toBeVisible();
});
it('reports a failed output move without claiming playback was updated',async()=>{
 const services=createFixtureServices({screen:'audio',state:'populated',theme:'dark'});
 vi.spyOn(services.backend,'moveSession').mockRejectedValueOnce({message:'The player window could not be verified.'});
 const {user}=renderApp({services,initialNavigation:{route:'audio'}});await user.click(screen.getByRole('button',{name:'Picture & sound'}));
 const panel=await screen.findByRole('dialog',{name:'Picture & sound'});const apply=within(panel).getByRole('button',{name:'Apply to playback'});await waitFor(()=>expect(apply).toBeEnabled());await user.click(apply);
 expect(await within(panel).findByRole('alert')).toHaveTextContent('The player window could not be verified.');expect(within(panel).queryByText('Playback output updated')).not.toBeInTheDocument();expect(apply).toBeEnabled();
});
