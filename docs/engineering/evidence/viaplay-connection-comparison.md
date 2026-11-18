# Viaplay connection comparison — 2026-11-05

The user reported CloudFront's 403 page when launching Viaplay in Mediacenter 0.1.3. They authorized temporarily disabling NordVPN and its Kill Switch for a comparison, followed by restoration.

The same installed Google Chrome executable, disposable browser context, URL (`https://viaplay.fi/`), and probe were used in both cases. The probe inspected the actual document response and rendered page text, rather than just verifying that a browser window existed.

| Connection | Document status | Rendered result |
| --- | --- | --- |
| Existing United States NordVPN server | 403 | CloudFront: request could not be satisfied; request blocked |
| VPN disconnected, Kill Switch temporarily disabled | 200 | Redirect to `/fi-fi/`; Finnish Viaplay homepage, including `Kirjaudu` |

Both the bare and `www` hostnames also returned 403 in the earlier VPN-connected HTTP checks. Changing the catalog hostname is therefore not supported by this evidence.

The original United States VPN server and enabled Kill Switch were restored and verified immediately after the comparison. No user browser profile, cookie, or login was changed. Headless browser automation was used for this comparison; no account login or paid playback was tested.

This establishes that the tested VPN connection causes the observed access failure. It does not establish whether geography, the VPN IP address, or another connection-dependent rule is the provider's precise blocking criterion. Viaplay's official guidance says VPN connections are unsupported: https://help.viaplay.com/fi/viaplay-abroad-vpn/.

Mediacenter currently defaults every service to requiring VPN, including Viaplay. Its existing per-service setting can allow Viaplay to use the current connection, but that setting does not disconnect NordVPN.

The user subsequently explicitly authorized leaving VPN and Kill Switch disabled for watching Viaplay. Both were disabled and their resulting states verified. Auto-connect was not changed. Viaplay's persisted `vpn_required` preference was set to false, then the same setting was applied through the running application's accessible settings UI so that live state agreed with the database. Other service preferences were not changed. No restart or package update was needed.

The old managed Viaplay error window was closed after verifying its process belonged to the exact Mediacenter Viaplay profile. Clicking the actual Viaplay tile launched a new managed window on the computer display. Its title became `Suoratoisto – rajattomasti viihdettä | Viaplay`; a screenshot confirmed the Finnish homepage with its normal cookie-choice dialog and content behind it, rather than the 403 error. The window was left open for the user. No cookie choice, credentials, subscription action, or playback action was submitted. VPN and Kill Switch remain disabled with user approval; no automatic restoration was scheduled.

Local diagnostic scripts and screenshots are under `.tools/viaplay-*` (ignored). The service-switching tests from 0.1.3 verify window ownership and switching, not provider availability, authentication, or playback.
