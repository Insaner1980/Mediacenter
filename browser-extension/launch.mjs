const nonce = location.hash.slice(1);
const status = document.querySelector('#status');

try {
  const result = await chrome.runtime.sendMessage({ type: 'remoteWake', nonce });
  if (result?.error === 'not_enrolled') {
    status.textContent = 'Open the Mediacenter Remote extension to enroll this Chrome profile.';
  } else if (result?.sent) {
    setTimeout(() => { status.textContent = 'Mediacenter did not finish opening the service.'; }, 15_000);
  } else {
    throw Error('bridge_unavailable');
  }
} catch {
  status.textContent = 'Check that Mediacenter is running and try opening the service again.';
}
