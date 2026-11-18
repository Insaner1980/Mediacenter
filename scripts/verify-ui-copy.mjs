import { readdir, readFile } from 'node:fs/promises';
async function check(directory) {
  for (const entry of await readdir(directory,{withFileTypes:true})) {
    const file = `${directory}/${entry.name}`;
    if (entry.isDirectory()) { if (entry.name !== 'testing') await check(file); continue; }
    if (!/\.(tsx?|css)$/.test(file)) continue;
    const source = await readFile(file,'utf8');
    if (/[·•●⋯…]/u.test(source) || /border-radius\s*:\s*50%/.test(source)) throw new Error(`Added dot decoration in ${file}`);
  }
}
await check('src');
console.log('No forbidden dot decoration found in application source; visual review is still required');
