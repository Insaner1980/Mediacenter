import {render,screen} from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import {expect,it,vi} from 'vitest';
import {AppShell} from '../../src/ui/AppShell';
import {referenceSnapshot} from '../../src/testing/fixtures';
it('has one Settings action and contextual navigation without a footer',async()=>{
 const navigate=vi.fn(),settings=vi.fn();
 render(<AppShell snapshot={referenceSnapshot} route="video" onNavigate={navigate} onOpenSettings={settings}><h1>Videos</h1></AppShell>);
 expect(screen.getAllByRole('button',{name:'Settings',exact:true})).toHaveLength(1);
 expect(screen.getByRole('navigation',{name:'Main navigation'})).toBeVisible();
 expect(screen.getByRole('button',{name:'Videos',exact:true})).toHaveAttribute('aria-current','page');
 await userEvent.click(screen.getByRole('button',{name:'Home',exact:true}));
 expect(navigate).toHaveBeenCalledWith('home');
 expect(screen.queryByText('Controls on this computer')).not.toBeInTheDocument();
});
