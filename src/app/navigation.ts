export type MainScreen = 'home' | 'video' | 'audio' | 'photo' | 'drives' | 'settings';
export type AppRoute = MainScreen | 'streaming' | 'player';
export type MusicTab = 'library' | 'playlists' | 'queue';
export type SettingsSection = 'outputs' | 'vpn' | 'appearance' | 'diagnostics';
export interface NavigationState {route: AppRoute; musicTab: MusicTab; settingsSection: SettingsSection | null}
