// Keep launch URLs in sync with src-tauri/src/streaming/catalog.rs. Additional
// origins are exact, provider-specific destinations for browser sign-in/playback.
export const services = new Map([
  ['netflix', { name: 'Netflix', url: 'https://www.netflix.com/', origins: [] }],
  ['youtube', { name: 'YouTube', url: 'https://www.youtube.com/', origins: ['https://accounts.google.com'] }],
  ['hbo_max', { name: 'HBO Max', url: 'https://www.hbomax.com/', origins: ['https://play.hbomax.com'] }],
  ['viaplay', { name: 'Viaplay', url: 'https://viaplay.fi/', origins: [] }],
  ['mtv_katsomo', { name: 'MTV Katsomo', url: 'https://www.mtv.fi/', origins: [] }],
  ['ruutu', { name: 'Ruutu', url: 'https://www.ruutu.fi/', origins: [] }],
  ['yle_areena', { name: 'Yle Areena', url: 'https://areena.yle.fi/', origins: [] }],
  ['plex', { name: 'Plex', url: 'https://app.plex.tv/', origins: ['https://plex.tv', 'https://www.plex.tv'] }],
]);
