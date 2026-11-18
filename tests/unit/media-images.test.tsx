import {expect,it,vi} from 'vitest';
import {rasterUrl,createThumbnailLoader} from '../../src/features/library/mediaImages';
it('rejects active, invalid and oversized raster payloads',()=>{expect(rasterUrl({mime:'image/svg+xml',base64:'PHN2Zy8+'})).toBeNull();expect(rasterUrl({mime:'image/png',base64:'<bad>'})).toBeNull();expect(rasterUrl({mime:'image/png',base64:'a'.repeat(6*1024*1024+1)})).toBeNull();});
it('bounds decode concurrency and caches decoded images',async()=>{
 let count=0,max=0;const fetch=vi.fn(async()=>{count++;max=Math.max(max,count);await new Promise(r=>setTimeout(r,5));count--;return {mime:'image/png',base64:'AAAA'};});const load=createThumbnailLoader(fetch);
 await Promise.all(Array.from({length:70},(_,i)=>load(String(i),String(i))));expect(max).toBe(4);await load('69','69');expect(fetch).toHaveBeenCalledTimes(70);await load('0','0');expect(fetch).toHaveBeenCalledTimes(71);
});
