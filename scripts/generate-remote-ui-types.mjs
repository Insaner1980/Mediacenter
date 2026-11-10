// Renderer consumes the canonical wire DTOs; the IPC envelope lives separately.
import {readFileSync,writeFileSync} from 'node:fs';
const schema=JSON.parse(readFileSync(new URL('../protocol/remote/v1/schema.json',import.meta.url),'utf8'));
const used=new Set();
function type(s){
 if(s.$ref){const name=s.$ref.split('/').at(-1);used.add(name);return name;}
 if(s.const!==undefined)return JSON.stringify(s.const);
 if(s.enum)return s.enum.map(JSON.stringify).join(' | ');
 if(s.oneOf)return s.oneOf.map(type).join(' | ');
 if(s.anyOf)return s.anyOf.map(type).join(' | ');
 if(Array.isArray(s.type))return s.type.map(t=>type({...s,type:t})).join(' | ');
 if(s.type==='object')return `{ ${Object.entries(s.properties??{}).map(([k,v])=>`${JSON.stringify(k)}${s.required?.includes(k)?'':'?'}: ${type(v)}`).join('; ')} }`;
 if(s.type==='array')return `Array<${type(s.items)}>`;
 if(['string','boolean','null'].includes(s.type))return s.type;
 if(['integer','number'].includes(s.type))return 'number';
 throw Error(JSON.stringify(s));
}
for(const name of ['UiRequest','UiAck','UiTopic','ConfirmationPreview','Scope'])used.add(name);
let output='// Generated from protocol/remote/v1/schema.json by scripts/generate-remote-ui-types.mjs.\n';
for(const name of used)output+=`export type ${name} = ${type(schema.$defs[name])};\n`;
const path=new URL('../src/features/remote/protocol.generated.ts',import.meta.url);
if(process.argv.includes('--check')){if(readFileSync(path,'utf8')!==output)throw Error('Renderer wire DTOs are stale');}else writeFileSync(path,output);
