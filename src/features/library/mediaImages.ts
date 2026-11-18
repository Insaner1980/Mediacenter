import type {EncodedImage} from '../../app/contracts';
export function rasterUrl(image:EncodedImage|undefined|null):string|null {
 if(!image||!['image/jpeg','image/png'].includes(image.mime)||image.base64.length>6*1024*1024||!/^[A-Za-z0-9+/]*={0,2}$/.test(image.base64))return null;
 return `data:${image.mime};base64,${image.base64}`;
}
export function createThumbnailLoader(fetch:(id:string)=>Promise<EncodedImage>){
 const cache=new Map<string,string|null>(),pending=new Map<string,Promise<string|null>>();let running=0;const waiting:(()=>void)[]=[];
 return function load(key:string,id:string):Promise<string|null>{
  if(cache.has(key)){const value=cache.get(key)!;cache.delete(key);cache.set(key,value);return Promise.resolve(value);}
  const existing=pending.get(key);if(existing)return existing;
  const work=new Promise<string|null>(resolve=>{
   const start=()=>{
    running++;
    void Promise.resolve().then(()=>fetch(id)).then(rasterUrl).then(value=>{
     cache.set(key,value);
     if(cache.size>64)cache.delete(cache.keys().next().value!);
     return value;
    }).catch(()=>null).then(value=>{
     running--;
     pending.delete(key);
     waiting.shift()?.();
     resolve(value);
    });
   };
   if(running<4)start();else waiting.push(start);
  });pending.set(key,work);return work;
 };
}
