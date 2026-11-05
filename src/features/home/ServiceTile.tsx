import type { ServiceDefinition, ServiceId } from './homeModel';
const classes: Record<ServiceId,string> = {netflix:'netflix',youtube:'youtube',hbo_max:'hbo',viaplay:'viaplay',mtv_katsomo:'katsomo',ruutu:'ruutu',yle_areena:'areena',plex:'plex'};
export function ServiceTile({service,onOpen}: {service:ServiceDefinition;onOpen:(id:ServiceId)=>void}) {
  return <button type="button" className={`service ${classes[service.id]}`} aria-label={service.name} onClick={()=>onOpen(service.id)}><img src={`/logos/${service.logoAsset}`} alt={service.name} draggable={false}/></button>;
}
