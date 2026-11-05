import type { ReactNode } from 'react';
const paths: Record<string, ReactNode> = {
  video: <><rect x="3" y="4" width="18" height="16" rx="3"/><path d="m10 8 6 4-6 4z"/></>,
  music: <><path d="M9 18V5l11-2v13M9 8l11-2"/><path d="M9 16H5a2 2 0 0 0-2 2v1a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2m11-5h-4a2 2 0 0 0-2 2v1a2 2 0 0 0 2 2h2a2 2 0 0 0 2-2"/></>,
  photos: <><rect x="3" y="3" width="18" height="18" rx="3"/><path d="m3 17 5-5 5 5 3-3 5 5M8 7v2m-1-1h2"/></>,
  drive: <><path d="m3 14 3-10h12l3 10"/><rect x="3" y="14" width="18" height="7" rx="2"/><path d="M16 17.5h2"/></>,
  settings: <><path d="M3 7h8m5 0h5M3 17h3m5 0h10"/><rect x="11" y="4" width="5" height="6" rx="1"/><rect x="6" y="14" width="5" height="6" rx="1"/></>,
  shield: <><path d="m12 3 8 4v5c0 5-8 9-8 9s-8-4-8-9V7zM8.5 12l2.5 2.5 4.5-5"/></>,
  display: <><rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8m-4-4v4"/></>,
  laptop: <><path d="M4 16V4h16v12M2 16h20l-2 4H4z"/></>,
};
export function Icon({name}: {name:string}) { return <svg className="icon" viewBox="0 0 24 24" aria-hidden="true">{paths[name]}</svg>; }
