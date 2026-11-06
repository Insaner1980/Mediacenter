import {test,expect} from '@playwright/test';
test('theme selection persists across restart and navigation',async({page})=>{
  await page.goto('/?fixture=reference');await page.getByRole('button',{name:'Settings'}).click();
  await page.getByRole('button',{name:'Light',exact:true}).click();
  await expect(page.locator('html')).toHaveAttribute('data-theme','light');
  await page.getByRole('button',{name:'Home',exact:true}).click();
  await expect(page.getByRole('button',{name:'Netflix',exact:true})).toBeVisible();
  await page.reload();await expect(page.locator('html')).toHaveAttribute('data-theme','light');
});
