import type {ButtonHTMLAttributes} from 'react';
import {Icon} from './Icons';
export function IconButton({label,icon,...props}:ButtonHTMLAttributes<HTMLButtonElement>&{label:string;icon:string}){return <button {...props} type="button" className={`icon-button ${props.className??''}`} aria-label={label} title={label}><Icon name={icon}/></button>;}
