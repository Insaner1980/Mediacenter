import {act,render,screen} from '@testing-library/react';
import {afterEach,expect,it,vi} from 'vitest';
import {PairingView} from '../../src/features/remote/PairingView';
afterEach(()=>vi.useRealTimers());
it('renders a local QR and manual phrase ceremony, then removes every secret on expiry',()=>{
 vi.useFakeTimers();const onExpire=vi.fn();render(<PairingView pairing={{attemptId:'attempt',qrRows:['111','101','111'],addresses:['192.168.1.4'],port:47832,manualCode:'ABCDEFGH2345',verificationWords:['able','about','absent','acid','across','act'],expiresInMs:2000}} deadline={performance.now()+2000} onExpire={onExpire}/>);
 expect(screen.getByRole('img',{name:'Pairing QR code'}).tagName.toLowerCase()).toBe('svg');expect(screen.getByText(/compare all six words/i)).toBeInTheDocument();expect(screen.getByText('ABCDEFGH2345')).toBeInTheDocument();expect(screen.getByText(/2 seconds/)).toBeInTheDocument();
 act(()=>vi.advanceTimersByTime(2100));expect(screen.getByText(/Pairing code expired/)).toBeInTheDocument();expect(screen.queryByRole('img')).not.toBeInTheDocument();expect(screen.queryByText('ABCDEFGH2345')).not.toBeInTheDocument();expect(onExpire).toHaveBeenCalledTimes(1);
});
