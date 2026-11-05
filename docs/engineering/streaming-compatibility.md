# Streaming compatibility — 5 November 2026

No row below proves compatibility for all accounts, programmes, regions or resolutions. Browser launch is distinct from actual playback. Installed Chrome: 157.0.8103.38. NordVPN country during the YouTube test: United States. Kill Switch was enabled afterward with explicit user approval.

| Service | Test date | Browser | Account/subscription prerequisite | Login | Actual playback | VPN country | Observed response | Computer | HDMI picture | Audio | Subtitles | Measured quality |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Netflix | not tested | not tested | requires account | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |
| YouTube | 2026-11-05 | Chrome 157.0.8103.38 | ordinary public video; existing browser session | existing login; fresh login not tested | passed for the public test title below | United States | video timer advanced to 1:07/10:34 | passed at browser-player level | not tested | audible output not tested | player reported unavailable | not measured |
| HBO Max | 2026-11-05, user report | managed Chrome; installed Chrome 157.0.8103.38 | requires account; credentials not inspected | initial user failure; later diagnostic reached sign-in form; authenticated login not tested | not reached | United States shown in screenshot | Initial Sign In error; later not reproduced in managed profile; cause unconfirmed | error page visible | not tested | not tested | not tested | not measured |
| Viaplay | not tested | not tested | requires account for chosen protected title | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |
| MTV Katsomo | not tested | not tested | title-dependent; not assessed | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |
| Ruutu | not tested | not tested | title-dependent; not assessed | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |
| Yle Areena | not tested | not tested | title-dependent; not assessed | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |
| Plex | not tested | not tested | chosen library/service not assessed | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested | not tested |

YouTube title: [Big Buck Bunny 60fps 4K - Official Blender Foundation Short Film](https://www.youtube.com/watch?v=aqz-KE-bpKQ), Blender. “4K” in the title is not a measurement of delivered quality. A public video does not satisfy the protected DRM title requirement. Browser automation was used only for this compatibility inspection, not as an application playback integration.

The TV was subsequently recognized by XRandR. Chrome blank-window placement is recorded separately in `evidence/chrome-window-preflight.json`; it does not establish service playback on the TV. No HDMI audio sink is currently exposed, despite an available HDMI card port.

## HBO Max sign-in investigation — 5 November 2026

The user reported that clicking Sign In in Mediacenter's managed HBO Max Chrome window led to the generic error above. The screenshot shows the HBO Max authentication route and a United States VPN observation. No credentials, authentication URL query parameters, or profile data are copied into this record.

The launch catalog uses `https://www.hbomax.com/`; Chrome starts with a dedicated profile and ordinary app-window flags, without security-disabling flags. The installed Chrome reports 157.0.8103.38. HBO's [sign-in help](https://help.hbomax.com/mm-en/answer/detail/000002552) lists HBOMax.com/signin and recommends browser comparisons; its specific incognito advice concerns TV-code failures. An incognito comparison here is our diagnostic test, not a confirmed fix for this error. Its [Finnish VPN help](https://help.hbomax.com/fi-fi/Answer/Detail/000002566) documents VPN restrictions, but this screenshot does not show HBO's explicit VPN-detection message. Neither an app-window defect nor a VPN cause is established.

Next comparison requested: the same sign-in page in ordinary Chrome incognito, keeping the VPN connection unchanged. No VPN settings, Kill Switch, browser data, display settings or application code were changed for this investigation.

Follow-up: the user confirmed that both ordinary Chrome and incognito work, including the homepage Sign In button, with the VPN unchanged. A private-display comparison using installed Chrome reached an email field in app mode with a fresh profile. The same test using the closed existing Mediacenter HBO profile also reached an email field after the authentication page settled. No credentials were entered, no profile was reset, and no VPN setting was changed. The original failure was not reproduced; an application-code fix is not justified by this evidence. Browser automation flags are a limitation of the comparison. See [sanitized diagnostic evidence](evidence/hbo-login-diagnostic.json). The user should retry the ordinary Mediacenter flow; actual authenticated login and playback remain untested.
