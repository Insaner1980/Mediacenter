import { readdir, readFile } from 'node:fs/promises';

function withoutCssComments(source) {
  const parts = [];
  let start = 0;
  let quote = '';
  for (let position = 0; position < source.length; position++) {
    const char = source[position];
    if (char === '\\') { position++; continue; }
    if (quote) {
      if (char === quote) quote = '';
      continue;
    }
    if (char === '"' || char === "'") { quote = char; continue; }
    if (!source.startsWith('/*', position)) continue;
    parts.push(source.slice(start, position), ' ');
    const end = source.indexOf('*/', position + 2);
    if (end < 0) return parts.join('');
    start = end + 2;
    position = start - 1;
  }
  parts.push(source.slice(start));
  return parts.join('');
}

async function check(directory) {
  for (const entry of await readdir(directory,{withFileTypes:true})) {
    const file = `${directory}/${entry.name}`;
    if (entry.isDirectory()) { if (entry.name !== 'testing') await check(file); continue; }
    if (!/\.(tsx?|css)$/.test(file)) continue;
    const source = await readFile(file,'utf8');
    const declarations = file.endsWith('.css') ? withoutCssComments(source) : source;
    if (/[·•●⋯…]/u.test(source) || /border-radius\s*:\s*50%/.test(declarations)) {
      throw new Error(`Added dot decoration in ${file}`);
    }
  }
}
await check('src');
console.log('No forbidden dot decoration found in application source; visual review is still required');
