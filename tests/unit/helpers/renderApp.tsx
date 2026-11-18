import {render} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {MainApp} from '../../../src/app/App';
import {AppServicesProvider,type AppServices} from '../../../src/app/AppServices';
import type {NavigationState} from '../../../src/app/navigation';
export function renderApp({services,initialNavigation}:{services:AppServices;initialNavigation?:Partial<NavigationState>}){return {...render(<AppServicesProvider services={services}><MainApp initialNavigation={initialNavigation}/></AppServicesProvider>),user:userEvent.setup()};}
