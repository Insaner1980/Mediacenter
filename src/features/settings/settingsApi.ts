import {invoke} from '@tauri-apps/api/core';
import type {DiagnosticReport} from '../../app/contracts';
export const settingsApi={preview:()=>invoke<DiagnosticReport>('get_diagnostics'),exportReport:()=>invoke<boolean>('export_diagnostics')};
