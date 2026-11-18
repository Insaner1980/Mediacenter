import {useEffect,useReducer} from 'react';
import type {VpnSnapshot} from '../../app/contracts';
export function currentObservation(vpn:VpnSnapshot,now:number):VpnSnapshot {
  if(vpn.observedAtMs>0 && now>=vpn.observedAtMs && now-vpn.observedAtMs<10_000)return vpn;
  return {...vpn,state:'unknown',country:null,killSwitch:'unknown',errorCode:'observation_stale'};
}
/** Expire a snapshot even when the next native refresh/event never arrives. */
export function useVpnObservation(vpn:VpnSnapshot):VpnSnapshot {
  const [,tick]=useReducer((value:number)=>value+1,0);
  useEffect(()=>{
    const timer=window.setInterval(tick,250);
    window.addEventListener('focus',tick);document.addEventListener('visibilitychange',tick);
    return()=>{window.clearInterval(timer);window.removeEventListener('focus',tick);document.removeEventListener('visibilitychange',tick);};
  },[]);
  return currentObservation(vpn,Date.now());
}
