import {createContext} from 'react';
// Remote route entry may read existing libraries, but never perform local setup.
export const RemoteNavigationOrigin=createContext(false);
export type MainScreen = 'home' | 'video' | 'audio' | 'photo' | 'drives' | 'settings';
export type AppRoute = MainScreen | 'streaming';
export type MusicTab = 'library' | 'playlists' | 'queue';
export type SettingsSection = 'outputs' | 'vpn' | 'appearance' | 'diagnostics';
export interface NavigationState {route: AppRoute; musicTab: MusicTab; settingsSection: SettingsSection | null}

export const remoteRouteFor={home:'home',video:'videos',audio:'music',photo:'photos',drives:'settings',settings:'settings',streaming:'services'} as const;
