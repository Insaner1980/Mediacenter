import {test,expect} from '@playwright/test';
import {readFile} from 'node:fs/promises';
test('all_eight_logo_tiles_have_single_visible_brand',async({page})=>{
  await page.goto('/?fixture=reference');
  const tiles=page.locator('.service'); await expect(tiles).toHaveCount(8);
  for(const tile of await tiles.all()) {
    await expect(tile).toHaveText(''); await expect(tile.locator('img')).toBeVisible();
    expect(await tile.locator('img').evaluate((img:HTMLImageElement)=>img.naturalWidth>0)).toBe(true);
  }
});
test('narrow_window_has_no_horizontal_overflow',async({page})=>{
  await page.setViewportSize({width:800,height:600});await page.goto('/?fixture=reference');
  await expect(page.locator('.service')).toHaveCount(8);
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBe(800);
});
test('keyboard_focus_does_not_change_grid_geometry',async({page})=>{
  await page.goto('/?fixture=reference');const tile=page.getByRole('button',{name:'Netflix',exact:true});
  await expect(tile).toBeVisible();const before=await tile.boundingBox();await tile.focus();
  expect(await tile.boundingBox()).toEqual(before);
});
test('release_does_not_include_demo_data',async()=>{
  const {readdir}=await import('node:fs/promises');
  for(const name of await readdir('dist/assets'))if(name.endsWith('.js'))expect(await readFile(`dist/assets/${name}`,'utf8')).not.toMatch(/Lapland Trip 2025|LG TV via HDMI|Evening playlist, 86 songs/);
});
