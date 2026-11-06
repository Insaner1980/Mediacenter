import {createContext,useContext,type ReactNode} from 'react';
export type NavigationGuard=(proceed:()=>void)=>void;
const Context=createContext<(guard:NavigationGuard|null)=>void>(()=>{});
export function NavigationGuardProvider({register,children}:{register:(guard:NavigationGuard|null)=>void;children:ReactNode}){return <Context.Provider value={register}>{children}</Context.Provider>;}
export function useNavigationGuard(){return useContext(Context);}
