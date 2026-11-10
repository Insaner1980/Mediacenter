import {PersonalMediaRow} from '../../src/features/home/PersonalMediaRow';
import {render,screen} from '@testing-library/react';
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
});
