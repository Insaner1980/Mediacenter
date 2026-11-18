import {useCallback, useEffect, useRef, useState} from 'react';
import type {RemoteSettingsSnapshot, RemotePairingDisplay, RemoteDevice} from '../../app/contracts';
import {errorMessage} from '../../app/backend';
import {remoteApi} from './remoteApi';
import {PairingView} from './PairingView';

export function RemoteSettings({api = remoteApi}: {api?: typeof remoteApi}) {
  const [settings, setSettings] = useState<RemoteSettingsSnapshot | null>(null);
  const [network, setNetwork] = useState('');
  const [port, setPort] = useState('');
  const [devices, setDevices] = useState<RemoteDevice[]>([]);
  const [pairing, setPairing] = useState<{api: typeof remoteApi; display: RemotePairingDisplay; deadline: number} | null>(null);
  const [expired, setExpired] = useState(false);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [revoke, setRevoke] = useState<RemoteDevice | null>(null);
  const owner = useRef<symbol | null>(null);
  const lock = useRef<symbol | null>(null);
  const activeAttempt = useRef<string | null>(null);
  const dirty = useRef(false);
  // A read issued before a mutation must not erase that mutation's result.
  const epoch = useRef(0);

  const receive = useCallback((value: RemoteSettingsSnapshot) => {
    setSettings(value);
    if (!dirty.current) {
      setNetwork(value.interfaceName ?? '');
      setPort(String(value.port));
    }
    if (activeAttempt.current && value.pairingAttemptId !== activeAttempt.current) {
      activeAttempt.current = null;
      setPairing(null);
      setExpired(false);
    }
  }, []);

  useEffect(() => {
    const token = Symbol();
    owner.current = token;
    lock.current = null;
    let refreshing = false;
    const refresh = async () => {
      if (refreshing) return;
      refreshing = true;
      const issuedEpoch = epoch.current;
      try {
        const [value, phones] = await Promise.all([api.settings(), api.devices()]);
        if (owner.current === token && issuedEpoch === epoch.current) {
          receive(value);
          setDevices(phones);
          if (lock.current !== token) setBusy(false);
        }
      } catch (e) {
        if (owner.current === token && issuedEpoch === epoch.current) setError(errorMessage(e));
      } finally {
        refreshing = false;
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => {
      owner.current = null;
      epoch.current++;
      clearInterval(timer);
      const id = activeAttempt.current;
      activeAttempt.current = null;
      if (id) void api.cancelPairing(id).catch(() => {});
    };
  }, [api, receive]);

  async function run(work: (token: symbol) => Promise<void>) {
    const token = owner.current;
    if (!token || lock.current) return;
    lock.current = token;
    epoch.current++;
    setBusy(true);
    setError('');
    try {
      await work(token);
    } catch (e) {
      if (owner.current === token) setError(errorMessage(e));
    } finally {
      if (owner.current === token) {
        epoch.current++;
        lock.current = null;
        setBusy(false);
      }
    }
  }

  const expire = useCallback(() => {
    const id = activeAttempt.current;
    activeAttempt.current = null;
    epoch.current++;
    setPairing(null);
    setExpired(true);
    if (id) void api.cancelPairing(id).catch(() => {});
  }, [api]);

  async function changeEnabled(token: symbol) {
    if (!settings) return;
    const id = activeAttempt.current;
    activeAttempt.current = null;
    setPairing(null);
    setExpired(false);
    if (id) void api.cancelPairing(id).catch(() => {});
    const value = await api.setEnabled({revision: settings.revision, enabled: !settings.enabled, interfaceName: network || null, port: Number(port)});
    if (owner.current === token) {
      dirty.current = false;
      receive(value);
    }
  }

  async function beginPairing(token: symbol) {
    const previous = activeAttempt.current;
    activeAttempt.current = null;
    setPairing(null);
    if (previous) await api.cancelPairing(previous);
    const started = performance.now();
    const display = await api.beginPairing();
    if (owner.current !== token) {
      await api.cancelPairing(display.attemptId);
      return;
    }
    epoch.current++;
    activeAttempt.current = display.attemptId;
    setExpired(false);
    setPairing({api, display, deadline: started + display.expiresInMs});
  }

  const validPort = /^\d+$/.test(port) && Number(port) > 0 && Number(port) <= 65535;
  return <section className="remote-settings" aria-label="Remote control">
    {settings ? <>
      <p role="status">{settings.status}</p>
      {settings.requiresRepairing && <p className="message">Mediacenter's certificate was replaced. Pair your phones again.</p>}
      <label className="field"><span>Local network</span><select aria-label="Local network" value={network} disabled={busy || settings.enabled} onChange={e => {dirty.current = true; setNetwork(e.target.value);}}>
        <option value="">Choose a trusted local network</option>
        {settings.interfaces.map(item => <option key={item.name} value={item.name}>{item.label}: {item.addresses.join(', ')}</option>)}
      </select></label>
      <label className="field"><span>Port</span><input aria-label="Port" type="number" min="1" max="65535" value={port} disabled={busy || settings.enabled} onChange={e => {dirty.current = true; setPort(e.target.value);}}/></label>
      <p>Use the same trusted local network on your phone. If the port is occupied, choose another port before enabling.</p>
      <button className="control" disabled={busy || (!settings.enabled && (!network || !validPort))} onClick={() => void run(changeEnabled)}>{settings.enabled ? 'Disable remote control' : 'Enable remote control'}</button>
      {settings.listening && <button className="control" disabled={busy} onClick={() => void run(beginPairing)}>Pair a phone</button>}
      {pairing && pairing.api === api && <>
        <PairingView key={pairing.display.attemptId} pairing={pairing.display} deadline={pairing.deadline} onExpire={expire}/>
        <button className="control" disabled={busy} onClick={() => void run(async () => {
          const id = activeAttempt.current;
          activeAttempt.current = null;
          setPairing(null);
          if (id) await api.cancelPairing(id);
        })}>Cancel pairing</button>
      </>}
      {expired && <p role="status">Pairing code expired. Choose Pair a phone for a new code.</p>}
      <h3>Paired phones</h3>
      {devices.length === 0 ? <p>No paired phones.</p> : devices.map(device => <div className="remote-device" key={device.deviceId}>
        <span>{device.label}</span><button className="control" disabled={busy} onClick={() => setRevoke(device)}>Revoke {device.label}</button>
      </div>)}
      {revoke && <div role="group" aria-label="Confirm revocation">
        <p>Revoke {revoke.label}? This phone will disconnect and must pair again.</p>
        <button className="control" disabled={busy} onClick={() => void run(async token => {
          await api.revokeDevice(revoke.deviceId);
          const phones = await api.devices();
          if (owner.current === token) {setDevices(phones); setRevoke(null);}
        })}>Confirm revocation</button>
        <button className="control" disabled={busy} onClick={() => setRevoke(null)}>Keep phone</button>
      </div>}
    </> : <p role="status">Checking remote control</p>}
    {error && <p role="alert" className="message">{error}</p>}
  </section>;
}
