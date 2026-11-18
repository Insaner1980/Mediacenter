import { services } from './service-catalog.mjs';

const setup = document.querySelector('#setup');
const kind = document.querySelector('#kind');
const serviceChoice = document.querySelector('#service-choice');
const service = document.querySelector('#service');
const status = document.querySelector('#status');
const enroll = document.querySelector('#enroll');
const unenroll = document.querySelector('#unenroll');
const adoptPopup = document.querySelector('#adopt-popup');

for (const [id, { name }] of services) {
  if (id !== 'hbo_max') service.add(new Option(name, id));
}
kind.addEventListener('change', () => {
  serviceChoice.hidden = kind.value !== 'managed';
  service.required = kind.value === 'managed';
});

function show(result) {
  if (!result || result.error) {
    status.textContent = 'Mediacenter enrollment is unavailable.';
    return;
  }
  setup.hidden = result.enrolled;
  unenroll.hidden = !result.enrolled;
  adoptPopup.hidden = !result.canAdoptPopup;
  status.textContent = result.connected ? 'Connected to Mediacenter.'
    : result.enrolled ? 'Enrolled. Waiting for Mediacenter.' : 'This profile is not enrolled.';
}

async function refresh() {
  try {
    show(await chrome.runtime.sendMessage({ type: 'remoteStatus' }));
  } catch {
    show();
  }
}

setup.addEventListener('submit', async event => {
  event.preventDefault();
  enroll.disabled = true;
  try {
    const selection = { kind: kind.value, ...(kind.value === 'managed' ? { service: service.value } : {}) };
    show(await chrome.runtime.sendMessage({ type: 'remoteEnroll', ...selection }));
  } catch {
    show();
  } finally {
    enroll.disabled = false;
  }
});
unenroll.addEventListener('click', async () => {
  try {
    show(await chrome.runtime.sendMessage({ type: 'remoteUnenroll' }));
  } catch {
    show();
  }
});
adoptPopup.addEventListener('click', async () => {
  try {
    show(await chrome.runtime.sendMessage({ type: 'remoteAdoptPopup' }));
  } catch {
    show();
  }
});

refresh();
setInterval(refresh, 1_000);

// Task 17's disposable profile stores its feasibility grant separately.
window.probeEnrollment = () => chrome.runtime.sendMessage({ type: 'enroll' });
window.probeRun = () => chrome.runtime.sendMessage({ type: 'probe' });
