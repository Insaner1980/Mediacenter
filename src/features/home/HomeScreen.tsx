import type { HomeSnapshot, HomeAction } from './homeModel';
import { AppShell } from '../../ui/AppShell';
import { ServiceTile } from './ServiceTile';
import { PersonalMediaRow } from './PersonalMediaRow';
export function HomeScreen({snapshot,onAction}: {snapshot: HomeSnapshot; onAction: (action: HomeAction) => void}) {
  return <AppShell snapshot={snapshot}><div className="app-main">
    <section aria-labelledby="yours-title"><h2 className="section-heading" id="yours-title">Yours</h2><div className="own-media">
      {(['video','audio','photo','drives','settings'] as const).map(kind=><PersonalMediaRow key={kind} kind={kind} summary={kind==='settings' ? 'Connection and playback output' : snapshot.summaries[kind]} progress={kind==='video' ? snapshot.summaries.resumeProgress : null} onOpen={screen=>onAction({type:'navigate',screen})}/>)}</div></section>
    <section className="streaming" aria-labelledby="streaming-title"><h2 className="section-heading" id="streaming-title">Streaming</h2><div className="service-grid">{snapshot.services.map(service=><ServiceTile key={service.id} service={service} onOpen={serviceId=>onAction({type:'service',serviceId})}/>)}</div></section>
  </div></AppShell>;
}
