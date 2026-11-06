import {createContext,useContext,type ReactNode} from 'react';
import {isTauri} from '@tauri-apps/api/core';
import {backend,subscribeHome} from './backend';
import {musicApi} from '../features/music/musicApi';
import {photosApi} from '../features/photos/api';
import {drivesApi} from '../features/drives/api';
import {playerApi} from '../features/player/playerApi';
import {settingsApi} from '../features/settings/settingsApi';
export interface AppServices {backend:typeof backend;music:typeof musicApi;photos:typeof photosApi;drives:typeof drivesApi;player:typeof playerApi;settings:typeof settingsApi;subscribeHome:typeof subscribeHome;canUseMedia:boolean}
export const productionServices:AppServices={backend,music:musicApi,photos:photosApi,drives:drivesApi,player:playerApi,settings:settingsApi,subscribeHome,canUseMedia:isTauri()};
const Context=createContext(productionServices);
export function AppServicesProvider({services=productionServices,children}:{services?:AppServices;children:ReactNode}){return <Context.Provider value={services}>{children}</Context.Provider>;}
export function useAppServices(){return useContext(Context);}
