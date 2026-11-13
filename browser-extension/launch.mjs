const nonce = location.hash.slice(1);
const status = document.querySelector('#status');

try {
  const result = await chrome.runtime.sendMessage({ type: 'remoteWake', nonce });
  if (!result?.sent) throw Error('bridge_unavailable');
  setTimeout(() => { status.textContent = 'Mediacenter did not finish opening the service.'; }, 15_000);
} catch {
  status.textContent = 'Open the Mediacenter Remote extension to enroll this Chrome profile.';
}
