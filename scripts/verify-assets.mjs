import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
const provenance = JSON.parse(await readFile('public/logos/provenance.json','utf8'));
for (const asset of provenance.assets) {
  const bytes = await readFile(`public/logos/${asset.file}`);
  if (createHash('sha256').update(bytes).digest('hex') !== asset.sha256) throw new Error(`Modified official logo: ${asset.file}`);
}
const fonts = JSON.parse(await readFile('public/fonts/provenance.json','utf8'));
for (const asset of fonts.files) {
  if (createHash('sha256').update(await readFile(`public/fonts/${asset.file}`)).digest('hex') !== asset.sha256) throw new Error(`Modified font: ${asset.file}`);
}
console.log(`Verified ${provenance.assets.length} official logos and ${fonts.files.length} local fonts`);
