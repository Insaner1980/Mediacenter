import {VideosScreen} from '../../src/features/videos/VideosScreen';
import {AppServicesProvider,productionServices} from '../../src/app/AppServices';
import {createFocusRegistry} from '../../src/features/remote/focusRegistry';
import type {MediaItem} from '../../src/app/contracts';
import {PersonalMediaRow} from '../../src/features/home/PersonalMediaRow';
import {render,screen,waitFor} from '@testing-library/react';
import {describe,it,expect} from 'vitest';
import {Sidebar} from '../../src/ui/Sidebar';
import {VideoCard} from '../../src/features/videos/VideoCard';

describe('remote semantic registration',()=>{
 it('registers stable sidebar actions but excludes local drive setup',()=>{
  render(<Sidebar route="home" onNavigate={()=>{}}/>);
  expect(screen.getByRole('button',{name:'Videos'})).toHaveAttribute('data-remote-id','sidebar-video');
  expect(screen.getByRole('button',{name:'Drives & USB'})).not.toHaveAttribute('data-remote-id');
 });
 it('exposes a typed media action rather than unrestricted local click authority',()=>{
  render(<VideoCard item={{id:'movie-1',displayName:'Movie',availability:'available',durationMs:1000,resumePositionMs:0} as never} onPlay={()=>{}}/>);
  expect(screen.getByRole('button',{name:'Play Movie'})).toHaveAttribute('data-remote-media','movie-1');
 });
 it('registers the actual home library rows when the sidebar is absent',()=>{
  render(<PersonalMediaRow kind="video" onOpen={()=>{}}/>);expect(screen.getByRole('button',{name:'Videos'})).toHaveAttribute('data-remote-id','home-video');
 });
 it('keeps actual video cards in their grid section and reaches pagination vertically',async()=>{
  const items:MediaItem[]=['a','b','c'].map(id=>({id,displayName:id,kind:'video',sourceId:'source',availability:'available',durationMs:1000,resumePositionMs:0,thumbnailId:null}));
  const services={...productionServices,canUseMedia:true,backend:{...productionServices.backend,ensureDefaultSource:async()=>null,sources:async()=>[],queryMedia:async()=>({items,nextCursor:'next'})}};
  render(<AppServicesProvider services={services}><VideosScreen onPlay={async()=>{}}/></AppServicesProvider>);await waitFor(()=>expect(screen.getByRole('button',{name:'Play a'})).toBeEnabled());
  for(const [index,id] of ['a','b','c'].entries())screen.getByRole('button',{name:`Play ${id}`}).getBoundingClientRect=()=>({left:index%2*100,top:100+Math.floor(index/2)*100,width:80,height:80} as DOMRect);
  const registry=createFocusRegistry();registry.configure('1','videos');screen.getByRole('button',{name:'Play a'}).focus();registry.sync();registry.move('right');expect(screen.getByRole('button',{name:'Play b'})).toHaveFocus();registry.move('down');expect(screen.getByRole('button',{name:'Play c'})).toHaveFocus();registry.move('right');expect(screen.getByRole('button',{name:'Play c'})).toHaveFocus();registry.move('down');expect(screen.getByRole('button',{name:'Next page'})).toHaveFocus();registry.move('up');expect(screen.getByRole('button',{name:'Play c'})).toHaveFocus();
 });
});
