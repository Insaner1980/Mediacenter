import {invoke} from '@tauri-apps/api/core';
import type {EncodedImage,PhotoImage,PhotoFrame,SlideshowAction,SlideshowRequest,SlideshowSnapshot} from '../../app/contracts';
export const photosApi = {
  start:(request:SlideshowRequest)=>invoke<SlideshowSnapshot>('start_slideshow',{request}),
  control:(action:SlideshowAction)=>invoke<SlideshowSnapshot>('control_slideshow',{action}),
  snapshot:()=>invoke<SlideshowSnapshot>('get_slideshow'),
  frame:()=>invoke<PhotoFrame|null>('get_presentation_frame'),
  image:(mediaId:string)=>invoke<PhotoImage>('get_photo_image',{mediaId}),
};
export function rasterUrl(image:EncodedImage|undefined|null):string|null {
  if(!image || !['image/jpeg','image/png'].includes(image.mime) || image.base64.length>6*1024*1024 || !/^[A-Za-z0-9+/]*={0,2}$/.test(image.base64))return null;
  return `data:${image.mime};base64,${image.base64}`;
}
