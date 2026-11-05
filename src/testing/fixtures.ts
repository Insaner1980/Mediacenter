import type { HomeSnapshot, ServiceDefinition } from '../features/home/homeModel';
const services: ServiceDefinition[] = [
  {id:'netflix',name:'Netflix',logoAsset:'netflix.svg',tileColor:'#080808',vpnRequired:true},
  {id:'youtube',name:'YouTube',logoAsset:'youtube.png',tileColor:'#f90018',vpnRequired:true},
  {id:'hbo_max',name:'HBO Max',logoAsset:'hbo-max.png',tileColor:'#46018a',vpnRequired:true},
  {id:'viaplay',name:'Viaplay',logoAsset:'viaplay.svg',tileColor:'#600c41',vpnRequired:true},
  {id:'mtv_katsomo',name:'MTV Katsomo',logoAsset:'mtv-katsomo.svg',tileColor:'#4e0710',vpnRequired:true},
  {id:'ruutu',name:'Ruutu',logoAsset:'ruutu.svg',tileColor:'#063f40',vpnRequired:true},
  {id:'yle_areena',name:'Yle Areena',logoAsset:'yle-areena.svg',tileColor:'#00796f',vpnRequired:true},
  {id:'plex',name:'Plex',logoAsset:'plex.svg',tileColor:'#151515',vpnRequired:true},
];
export const referenceSnapshot: HomeSnapshot = {revision:1,theme:'dark',vpn:{revision:0,errorCode:null,state:'connected',country:'United Kingdom',killSwitch:'enabled',observedAtMs:Date.now()},outputs:{revision:0,desired:{monitorId:null,audioSinkId:null},effective:{monitorId:null,audioSinkId:null},label:'LG TV via HDMI',state:'ready'},summaries:{video:'Resume Lapland Trip 2025',audio:'Evening playlist, 86 songs',photo:'Archipelago slideshow',drives:'2 TB drive, 64 GB USB stick',resumeProgress:.57},sessions:[],services};
