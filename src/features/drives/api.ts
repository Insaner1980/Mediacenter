import {invoke} from '@tauri-apps/api/core';
import {backend} from '../../app/backend';
export type EjectPreview={driveIds:string[];driveNames:string[];volumeIds:string[];mountedVolumes:number;activeUses:string[]};
export type EjectOutcome={driveIds:string[];status:'powered_off'|'unmounted'};
export const drivesApi={
 list:backend.drives,
 mount:(volumeId:string)=>invoke<{volumeId:string}>('mount_volume',{volumeId}),
 prepare:(driveId:string)=>invoke<EjectPreview>('prepare_eject_drive',{driveId}),
 eject:(driveId:string,confirmedDriveIds:string[])=>invoke<EjectOutcome>('eject_drive',{driveId,confirmedDriveIds}),
};
