import {useEffect,useState} from 'react';
import type {RemotePairingDisplay} from '../../app/contracts';
export function PairingView({pairing,deadline,onExpire}:{pairing:RemotePairingDisplay;deadline:number;onExpire:()=>void}){
 const [remaining,setRemaining]=useState(()=>Math.max(0,Math.ceil((deadline-performance.now())/1000)));
 useEffect(()=>{let expired=false;const tick=()=>{const left=Math.max(0,Math.ceil((deadline-performance.now())/1000));setRemaining(left);if(left===0&&!expired){expired=true;onExpire();}};tick();const timer=setInterval(tick,250);return()=>clearInterval(timer);},[deadline,onExpire]);
 if(remaining===0)return <p role="status">Pairing code expired. Choose Pair a phone for a new code.</p>;
 const size=pairing.qrRows.length;
 return <div className="remote-pairing"><p>In Remote, choose Add Mediacenter and scan this code.</p><svg className="pairing-qr" role="img" aria-label="Pairing QR code" viewBox={`0 0 ${size+8} ${size+8}`} shapeRendering="crispEdges"><rect width={size+8} height={size+8} fill="white"/>{pairing.qrRows.flatMap((row,y)=>[...row].map((cell,x)=>cell==='1'?<rect key={`${x},${y}`} x={x+4} y={y+4} width="1" height="1" fill="black"/>:null))}</svg><p role="status">Expires in {remaining} seconds</p><details><summary>Pair without a camera</summary><p>Enter an address and port in Remote. Compare all six words on the phone with these words. Confirm the match on the phone before entering the one-use code.</p><p>Address: {pairing.addresses.join(', ')} · Port: {pairing.port}</p><p className="verification-words">{pairing.verificationWords.join(' ')}</p><p>One-use code: <strong className="manual-code">{pairing.manualCode}</strong></p><p>If the words differ, cancel pairing.</p></details></div>;
}
