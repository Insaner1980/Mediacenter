import {useState} from 'react';
import {act,render,screen} from '@testing-library/react';
import {describe,expect,it,vi} from 'vitest';
import {Sidebar} from '../../src/ui/Sidebar';
import {Dialog} from '../../src/ui/Dialog';
import {LibraryToolbar} from '../../src/features/library/LibraryToolbar';
import {createFocusRegistry} from '../../src/features/remote/focusRegistry';
import {createRemoteNavigation,type RendererRequest} from '../../src/features/remote/remoteNavigation';
import type {UiRequest} from '../../src/features/remote/protocol.generated';
const nav={navigate:vi.fn(),back:vi.fn(),settle:async()=>{}};
function envelope(request:UiRequest):RendererRequest{return {dispatchId:'dispatch',requestId:'request',origin:'connection',ownerDeviceId:'phone',expiresAtMs:Date.now()+4000,request};}
describe('remote navigation',()=>{
 it('moves sidebar/list focus, scrolls, restores removed items and stays at list boundaries',()=>{
  const view=render(<><Sidebar route="home" onNavigate={()=>{}}/>{['a','b','c'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</>);
  const registry=createFocusRegistry();registry.configure('1','home');
  registry.move('down');expect(screen.getByRole('button',{name:'Home'})).toHaveFocus();registry.move('down');expect(screen.getByRole('button',{name:'Videos'})).toHaveFocus();
  const scroll=vi.fn();screen.getByText('a').scrollIntoView=scroll;registry.move('right');expect(screen.getByText('a')).toHaveFocus();expect(scroll).toHaveBeenCalledWith({block:'nearest',inline:'nearest'});
  registry.move('down');expect(screen.getByText('b')).toHaveFocus();
  view.rerender(<><Sidebar route="home" onNavigate={()=>{}}/>{['a','c'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</>);registry.sync();expect(screen.getByText('c')).toHaveFocus();registry.move('down');expect(screen.getByText('c')).toHaveFocus();
 });
 it('uses actual grid geometry for directional movement',()=>{
  render(<div data-remote-section="movies" data-remote-layout="grid">{['a','b','c','d'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>);
  for(const [i,id] of ['a','b','c','d'].entries())screen.getByText(id).getBoundingClientRect=()=>({left:(i%2)*100,top:Math.floor(i/2)*100,width:80,height:80} as DOMRect);
  const registry=createFocusRegistry();registry.configure('1','videos');registry.move('down');registry.move('right');expect(screen.getByText('b')).toHaveFocus();registry.move('down');expect(screen.getByText('d')).toHaveFocus();registry.move('left');expect(screen.getByText('c')).toHaveFocus();
 });
 it('rejects stale Activate after a dialog opens, traps focus and Back restores the opener',async()=>{
  const activate=vi.fn();function Example(){const [open,setOpen]=useState(false);return <><button data-remote-id="open" data-remote-group="content" data-remote-local="navigation" onClick={()=>setOpen(true)}>Open</button><Dialog remoteDismiss open={open} title="Modal" onRequestClose={()=>setOpen(false)}><button data-remote-id="danger" data-remote-group="modal" onClick={activate}>Danger</button></Dialog></>;}
  render(<Example/>);const registry=createFocusRegistry();registry.configure('1','home');registry.move('down');const before=registry.context();const handle=createRemoteNavigation();
  act(()=>screen.getByText('Open').click());
  await expect(handle(envelope({method:'ui.activate',params:{rendererGeneration:'1',focusRevision:before.focusRevision!,focusId:'open'}}),registry,nav)).rejects.toMatchObject({code:'stale_state'});
  registry.move('down');expect(screen.getByRole('dialog')).toContainElement(document.activeElement as HTMLElement);expect(activate).not.toHaveBeenCalled();
  const current=registry.sync();await act(()=>handle(envelope({method:'ui.back',params:{rendererGeneration:'1',focusRevision:current.focusRevision!,modalId:current.modalId}}),registry,nav));
  expect(screen.queryByRole('dialog')).not.toBeInTheDocument();expect(screen.getByText('Open')).toHaveFocus();
 });
 it('keeps committed search ordered, route-bound and rejects edits during local composition',async()=>{
  function Search(){const [query,setQuery]=useState('');return <LibraryToolbar kind="video" query={query} sourceId={null} sources={[]} onQueryChange={setQuery} onSourceChange={()=>{}} onRefresh={()=>{}}/>;}
  render(<Search/>);const registry=createFocusRegistry();registry.configure('1','videos');const handle=createRemoteNavigation();
  const search=(text:string,editSequence:string,route:'videos'|'music'='videos')=>handle(envelope({method:'ui.search',params:{rendererGeneration:'1',focusRevision:registry.sync().focusRevision!,route,text,editSequence}}),registry,nav);
  await act(()=>search('new','2'));expect(screen.getByRole('textbox')).toHaveValue('new');
  await expect(search('old','1')).rejects.toMatchObject({code:'stale_state'});expect(screen.getByRole('textbox')).toHaveValue('new');
  await expect(search('wrong route','3','music')).rejects.toMatchObject({code:'stale_state'});
  act(()=>screen.getByRole('textbox').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true})));
  await expect(search('interrupt','4')).rejects.toMatchObject({code:'stale_state'});
  act(()=>screen.getByRole('textbox').dispatchEvent(new CompositionEvent('compositionend',{bubbles:true})));
  await act(()=>search('committed','5'));expect(screen.getByRole('textbox')).toHaveValue('committed');
 });
 it('never clicks unregistered local authority through navigation',()=>{
  const local=vi.fn();render(<button data-remote-id="setup" data-remote-group="content" onClick={local}>Change setup</button>);const registry=createFocusRegistry();registry.configure('1','settings');registry.move('down');expect(()=>registry.activate('setup')).toThrow();expect(local).not.toHaveBeenCalled();
 });
 it('cannot dismiss a local-only setup dialog through remote Back',()=>{
  const setup=vi.fn();render(<Dialog open title="Pairing setup" onRequestClose={setup}><p>Private setup</p></Dialog>);const registry=createFocusRegistry();registry.configure('1','settings');expect(()=>registry.back()).toThrow();expect(setup).not.toHaveBeenCalled();
 });

 it('moves across horizontal tabs and down into their list using defined transitions',()=>{
  render(<><button data-remote-id="tab-library" data-remote-group="tabs" aria-selected="true">Library</button><button data-remote-id="tab-queue" data-remote-group="tabs">Queue</button><button data-remote-id="song" data-remote-group="content">Song</button></>);const registry=createFocusRegistry();registry.configure('1','music');registry.move('down');registry.move('right');expect(screen.getByText('Queue')).toHaveFocus();registry.move('down');expect(screen.getByText('Song')).toHaveFocus();registry.move('up');expect(screen.getByText('Library')).toHaveFocus();
 });

 it('rejects a known search field when its containing panel is hidden',()=>{
  render(<div style={{display:'none'}}><LibraryToolbar kind="video" query="" sources={[]} sourceId={null} onQueryChange={()=>{}} onSourceChange={()=>{}} onRefresh={()=>{}}/></div>);const registry=createFocusRegistry();registry.configure('1','videos');expect(()=>registry.search('videos','hidden')).toThrow();
 });

 it('visits adjacent unequal-width list rows without skipping aligned rows',()=>{
  render(<div data-remote-section="tracks" data-remote-layout="list">{['a','b','c'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>);
  for(const [i,id] of ['a','b','c'].entries())screen.getByText(id).getBoundingClientRect=()=>({left:0,top:i*100,width:id==='b'?240:80,height:40} as DOMRect);
  const registry=createFocusRegistry();registry.configure('1','music');registry.move('down');registry.move('down');expect(screen.getByText('b')).toHaveFocus();registry.move('down');expect(screen.getByText('c')).toHaveFocus();registry.move('up');expect(screen.getByText('b')).toHaveFocus();
 });
 it('stays within an incomplete grid row at horizontal boundaries',()=>{
  render(<div data-remote-section="movies" data-remote-layout="grid">{['a','b','c'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>);
  for(const [i,id] of ['a','b','c'].entries())screen.getByText(id).getBoundingClientRect=()=>({left:(i%2)*100,top:Math.floor(i/2)*100,width:80,height:80} as DOMRect);
  const registry=createFocusRegistry();registry.configure('1','videos');registry.move('down');registry.move('down');expect(screen.getByText('c')).toHaveFocus();registry.move('right');expect(screen.getByText('c')).toHaveFocus();registry.move('left');expect(screen.getByText('c')).toHaveFocus();registry.move('up');expect(screen.getByText('a')).toHaveFocus();
 });
 it('selects the adjacent grid row before considering horizontal alignment',()=>{
  render(<div data-remote-section="movies" data-remote-layout="grid">{['a','b','c'].map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>);
  for(const [i,id] of ['a','b','c'].entries())screen.getByText(id).getBoundingClientRect=()=>({left:id==='b'?300:0,top:i*100,width:80,height:80} as DOMRect);
  const registry=createFocusRegistry();registry.configure('1','videos');registry.move('down');registry.move('down');expect(screen.getByText('b')).toHaveFocus();
 });
 it('recovers the next stable survivor after multiple removals and a reorder',()=>{
  const rows=(ids:string[])=><div data-remote-section="tracks" data-remote-layout="list">{ids.map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>;
  const view=render(rows(['a','b','c','d']));const registry=createFocusRegistry();registry.configure('1','music');registry.move('down');registry.move('down');expect(screen.getByText('b')).toHaveFocus();
  view.rerender(rows(['c','d']));registry.sync();expect(screen.getByText('c')).toHaveFocus();view.rerender(rows(['a','b','c','d']));screen.getByText('b').focus();registry.sync();view.rerender(rows(['d','c']));registry.sync();expect(screen.getByText('c')).toHaveFocus();view.rerender(rows(['d']));registry.sync();expect(screen.getByText('d')).toHaveFocus();
 });
 it('uses a defined next section when an entire list disappears',()=>{
  const contents=(ids:string[])=><><Sidebar route="audio" onNavigate={()=>{}}/><div data-remote-section="search" data-remote-layout="row"><button data-remote-id="search" data-remote-group="content">Search</button></div><div data-remote-section="tracks" data-remote-layout="list">{ids.map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div><div data-remote-section="pages" data-remote-layout="row"><button data-remote-id="next" data-remote-group="content">Next page</button></div></>;
  const view=render(contents(['a','b']));const registry=createFocusRegistry();registry.configure('1','music');screen.getByText('b').focus();registry.sync();view.rerender(contents([]));registry.sync();expect(screen.getByText('Next page')).toHaveFocus();
 });
 it('recovers semantic order inside the modal and falls back inside that modal',()=>{
  const contents=(ids:string[])=><><button data-remote-id="outside" data-remote-group="content">Outside</button><Dialog remoteDismiss open title="Items" onRequestClose={()=>{}}><div data-remote-section="modal-items" data-remote-layout="list">{ids.map(id=><button key={id} data-remote-id={id} data-remote-group="modal">{id}</button>)}</div></Dialog></>;
  const view=render(contents(['a','b','c','d']));const registry=createFocusRegistry();registry.configure('1','music');screen.getByText('b').focus();registry.sync();view.rerender(contents(['c','d']));registry.sync();expect(screen.getByText('c')).toHaveFocus();view.rerender(contents([]));registry.sync();expect(screen.getByRole('button',{name:'Close'})).toHaveFocus();expect(screen.getByText('Outside')).not.toHaveFocus();
 });
 it('recovers backward in prior semantic order when no later item survives',()=>{
  const rows=(ids:string[])=><div data-remote-section="tracks" data-remote-layout="list">{ids.map(id=><button key={id} data-remote-id={id} data-remote-group="content">{id}</button>)}</div>;
  const view=render(rows(['a','b','c','d']));const registry=createFocusRegistry();registry.configure('1','music');screen.getByText('c').focus();registry.sync();view.rerender(rows(['b','a']));registry.sync();expect(screen.getByText('b')).toHaveFocus();
 });
});
