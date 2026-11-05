import { test, expect } from '@playwright/test';
import { readFile } from 'node:fs/promises';
// Compare layout against the original HTML, not a product-generated golden.
for (const theme of ['dark','light'] as const) {
  for (const width of [1280,1440,960]) {
    test(`home_${theme}_matches_reference_${width}`, async ({page,context},testInfo)=>{
      await page.setViewportSize({width,height:900});
      await page.addInitScript(t=>localStorage.setItem('mediacenter.theme',t),theme);
      await page.goto('/?fixture=reference');
      await expect(page.getByRole('button',{name:'Netflix',exact:true})).toBeVisible();
      await page.evaluate(()=>document.fonts.ready);
      await page.locator('.service img').evaluateAll(images=>Promise.all(images.map(image=>(image as HTMLImageElement).decode())));
      const reference=await context.newPage();
      await reference.setViewportSize({width,height:900});
      const fontCss=(await readFile('public/fonts/fonts.css','utf8')).replaceAll('/fonts/','http://127.0.0.1:1420/fonts/');
      await reference.route('https://fonts.googleapis.com/**',route=>route.fulfill({contentType:'text/css',body:fontCss}));
      await reference.goto('/docs/design/2026-11-05-mediacenter-colour-wall-desktop.html');
      await reference.evaluate(t=>{
        document.documentElement.dataset.theme=t;
        // Remove only the presentation surround and native-frame equivalent.
        const style=document.createElement('style');
        style.textContent='.presentation{width:100%;margin:0}.preview-heading,.preview-note{display:none}.app{border:0;border-radius:0;box-shadow:none}';
        document.head.append(style);
      },theme);
      await reference.evaluate(()=>document.fonts.ready);
      await reference.locator('.service img').evaluateAll(images=>Promise.all(images.map(image=>(image as HTMLImageElement).decode())));
      for (const selector of ['.app-header','.brand','.connections','.app-main','.section-heading','.media-row','.service','.app-footer']) {
        const geometry=async (p:typeof page)=>p.locator(selector).evaluateAll(elements=>elements.map(el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height};}));
        const actual=await geometry(page); const expected=await geometry(reference);
        expect(actual.length).toBe(expected.length);
        for(let i=0;i<actual.length;i++)for(const key of ['x','y','width','height'] as const)expect(Math.abs(actual[i][key]-expected[i][key]),`${selector}[${i}].${key}`).toBeLessThanOrEqual(2);
      }
      await page.locator('.app').screenshot({path:testInfo.outputPath(`${theme}-${width}.png`)});
      await reference.locator('.app').screenshot({path:testInfo.outputPath(`reference-${theme}-${width}.png`)});
      await reference.close();
    });
  }
}
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
