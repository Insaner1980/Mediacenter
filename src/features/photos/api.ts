import {invoke} from '@tauri-apps/api/core';
import type {PhotoImage,PhotoFrame,SlideshowAction,SlideshowRequest,SlideshowSnapshot} from '../../app/contracts';
export const photosApi = {
  start:(request:SlideshowRequest)=>invoke<SlideshowSnapshot>('start_slideshow',{request}),
  control:(action:SlideshowAction)=>invoke<SlideshowSnapshot>('control_slideshow',{action}),
  snapshot:()=>invoke<SlideshowSnapshot>('get_slideshow'),
  frame:()=>invoke<PhotoFrame|null>('get_presentation_frame'),
  image:(mediaId:string)=>invoke<PhotoImage>('get_photo_image',{mediaId}),
};
export {rasterUrl} from '../library/mediaImages';
