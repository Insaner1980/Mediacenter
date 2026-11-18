import {invoke} from '@tauri-apps/api/core';
import type {RemoteSettingsSnapshot,RemoteEnableRequest,RemotePairingDisplay,RemoteDevice} from '../../app/contracts';
export const remoteApi={
 settings:()=>invoke<RemoteSettingsSnapshot>('get_remote_settings'),
 setEnabled:(config:RemoteEnableRequest)=>invoke<RemoteSettingsSnapshot>('set_remote_enabled',{config}),
 beginPairing:()=>invoke<RemotePairingDisplay>('begin_remote_pairing'),
 cancelPairing:(attemptId:string)=>invoke<void>('cancel_remote_pairing',{attemptId}),
 devices:()=>invoke<RemoteDevice[]>('list_remote_devices'),
 revokeDevice:(deviceId:string)=>invoke<void>('revoke_remote_device',{deviceId}),
};
