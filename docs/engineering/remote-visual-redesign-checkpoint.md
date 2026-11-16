# Remote visual redesign — jatka tästä

Tallennettu **16.11.2026 klo22.03 (Europe/Helsinki)**. Käyttäjä pyysi keskeyttämään, koska kone pitää sulkea. **Älä aloita alusta. Tehtävät1–10 on toteutettu; tehtävä11 on kesken.** Täysi lopputarkistus ja lähdekoodin yhdistäminen ovat vielä tekemättä. Tämä muistio korvaa vanhemman `remote-control-checkpoint.md`-tiedoston tilannekuvan tämän ulkoasu-uudistuksen osalta.

## Käyttäjän voimassa oleva toimeksianto

Toteuta Clauden valmis suunnitelma tehtävä kerrallaan:

- Suunnitelma: `/home/emma/Dev/Remote/.worktrees/visual-redesign/docs/superpowers/plans/2026-11-16-remote-visual-redesign.md`.
- Sama käyttäjän alkuperäinen tiedosto: `/home/emma/Dev/Mediacenter/docs/superpowers/plans/2026-11-16-remote-visual-redesign.md`.
- Yksityiset mallikuvat: `/home/emma/Dev/Mediacenter/.superpowers/sdd/2026-11-16-remote-redesign/`. README luettiin ensimmäisenä. Avaa jokaisen uuden tehtävän kaikki kuvat ennen sen tiedostojen muokkausta.
- LG ja Mediacenter pysyvät tarkoituksella erilaisina. Roboto Flex on päätetty ja jo mukana. Älä kysy näitä päätöksiä tai jo annettuja lupia uudestaan.
- Ei palloerottimia, ei kuvakkeiden alatekstejä TV-/palveluriveihin, ei teknisiä selityksiä. Alkuperäinen **Remote**, paketti `com.emmah.colordock.remote`; ei erillistä Mediacenter Remotea.
- Visuaalinen muutos: ei ViewModel-/controller-/protokolla-/tallennusmuutoksia. Säilytä callbackit, guardit, saavutettavuus ja testitagit. AGENTS.md ei muutu.

## Työpuut ja säilytettävä aineisto

**Androidin toteutustyöpuu:** `/home/emma/Dev/Remote/.worktrees/visual-redesign`, haara `codex/remote-visual-redesign`.

**Alkuperäinen Android checkout:** `/home/emma/Dev/Remote`, haara `codex/initial-import`, edelleen suunnitelman tuonnissa86c5043. Älä tee toteutusta vahingossa sinne tai Mediacenterin Rust-koodiin. Lopuksi paikallinen fast-forward alkuperäiseen checkoutiin on vielä tekemättä; ei pushia.

**Työmuistio W:** `/home/emma/Dev/Remote/.worktrees/visual-redesign/.superpowers/sdd/2026-11-16-remote-visual-redesign/`.

Lue ensin **W/progress.md**: täysi tehtäväkohtainen ledger, päätökset perusteluineen, testit, commitit. Tehtävän valmistumisen todistaa `Task N: complete`, ei alun vanha pending-lista. `task-11-brief.md` sisältää nykyisen tehtävän tarkat arvot. Kaikki pienet työskriptit ja lähdemetatiedot ovat W:ssä; ne on jo ajettu, **älä aja edit.py-skriptejä uudelleen**. Keskeytyshetken muutoksista on lisäksi `W/task11-paused.patch`; uudet kaksi drawablea ovat normaalisti työpuussa. Älä poista W:tä ennen lopullista reviewta ja päätösten arkistointia.

Yksityiset asennus-/kuvatodisteet ovat mallikansiossa `installed/`; ne eivät kuulu julkiseen Gitiin. Säilytä käyttäjän muut dirty-tiedostot Mediacenterissä: streaming-compatibility.md, vanha simplified-concepts-kansio, viaplay-connection-comparison.md ja vanha sofa-control-suunnitelma.

## Valmis tähän asti

Tehtävät1–10 valmistuivat järjestyksessä. Viimeinen valmis commit **`dd1d77c621c22650ea360b5adc454feb3b91a5cb`**.

1. Väriroolit ja pintojen päivitys (91e4917).
2. Bundlattu Roboto Flex, typografia, muodot (b59da3a).
3. Mediacenterin omat listat, napit, valitsimet ja hakukentät (b709f09).
4. Jaettu suuntaohjain ilman vinosaumoja/varjoja (c87476c).
5. Laitevalitsimen MC-alalehti ja LG-dialogin tyyli (b513c62).
6. MC-otsikot ja alavalikko (2db0c31).
7. MC Remote -asettelu (7e5e40a), tyhjän toistokortin poistokorjaus5222fc8.
8. Kuuden rivin Settings ja otsikoiden päivitystoiminnot (c175ad5).
9. Kaikki kuusi asetusalasivua (d2659e5), VPN-rivien56dp-korjaus (a49f50a). **478 unit-testiä/0 virhettä**, assembleDebug/detekt/ktlint kunnossa. Oikean puhelimen kaikki kuusi tummaa alasivua katsottiin, tiivis VPN myös vieritettynä.
10. Kirjaston haku ensin, valintachipit, media96×54, kuvaruudukko3 saraketta/3dp-välit, lyhyet korvaavat otsikot8dp-sisennyksellä ja sivutus (dd1d77c). **31 kohdistettua testiä/0 virhettä**, assembleDebug/detekt/ktlint kunnossa. Uusi nimisisennyksen testi RED→GREEN. Täysi unit-sarja on viimeksi ajettu tehtävä9:ssä; tehtävä10:n jälkeen sitä ei ole vielä ajettu kokonaan.

## Kesken: tehtävä11 (LG Remote)

BASE **dd1d77c**. Kaikki tehtävän11 viisi viitekuvaa avattiin ennen muokkauksia. Toteutus on **tallennettu tiedostoihin mutta EI commitoitu eikä vielä hyväksytysti testattu**:

- `ui/RemoteButtons.kt`:64dp-pikkupainikkeet,24dp-kuvakeslotti,123 titleMedium, labelMedium/ellipsis, vaakanapit labelLarge, VOL/CH-tekstit/20dp-kuvakkeet, AdaptivePair pinoaa fontScale>=1.4.
- `ui/RemoteScreen.kt`: neutraali tilateksti ilman Canvas-palloa/värivakioita, Color Dock open_in_new,32dp-alamarginaali, VOL/CH-parametrit, iso teksti pinoaa>=1.4.
- `ui/ThemeSettings.kt`:48dp-reunustettu contrast-painike.
- `ui/TvAppsSheet.kt`: vain iconOnly-laatoille4dp-sisämarginaali/12dp-logoleikkaus sekä rivin oikean reunan häivytys.
- `ui/TargetSelector.kt`: LG-laitenimi yksirivinen ellipsis. Kolmen rivin Change device/nimi/tila -rakenne säilyy.
- Uudet viralliset Material Symbols `ic_contrast.xml`, `ic_open_in_new.xml`; URL/hashit W/task11-icons.json.
- Testimuutokset `PhoneUsabilityTest.kt`, `RemoteScreenTest.kt`.

**Testit nähtiin ensin epäonnistuvan:**5/5 RED lokissa `/tmp/remote-redesign-t11-red-size.log`: kaksi tasakorkeustestiä1.0/1.15, VOL/CH, yhden rivin pitkä laitenimi, tarkka1.4-pinoamisraja. Ensimmäinen versio korkeustesteistä oli väärin rajattu320dp-juureen ja ohitti kompaktin asettelun; se korjattiin käyttämään `DeviceConfigurationOverride.ForcedSize(412×900dp)` ja FontScalea ennen toteutusta. Käytä korjattua versiota.

**VIIMEINEN TARKISTUS EPÄONNISTUI:** `/tmp/remote-redesign-t11-green.log`. Main Kotlin kääntyi, mutta **Detekt: `TvAppsSheet.kt`, `TvAppTile`, LongMethod61>60**. Gradle keskeytti tämän vuoksi ennen testien/assemble/ktlint-gaten valmistumista. Älä merkitse tehtävää11 valmiiksi tai väitä vihreäksi. Ei näyttöä muista virheistä vielä.

**Ensimmäinen seuraava työ:** pienennä `TvAppTile`-funktiota erottamalla pieni presentation-helper (esim. artwork-modifier), muuttamatta behavioria tai LG-pickerin rakennetta. Ei laajaa suppressiota. Aja formatter loppuun ennen compile/testiä, sitten tehtävä11:n nimetyt testit + PhoneUsabilityTest, assembleDebug, detekt, ktlint. Korjaa mahdolliset oikeat virheet. Commit ja `task-done` vasta GREEN. Älä aja W/task11-edit.py:tä uudelleen.

Tehtävä11:n ledger-päätökset: hard constraint6 vaatii yksirivisen LG-nimen; Task14 vaatii pinoamisen täsmälleen1.4:ssä, vaikka vanha vertailu oli `>1.4`. Kiinteä vertical-korkeus vapautuu edelleen vasta yli1.4;1.4 käyttää jo pinottua näkymää. Arvioi ja säilytä perustelut ledgerissä.

## Seuraavat tehtävät

- **12:** Wallpapers/Color Dock vain teemamuutosten tarkistus molemmissa teemoissa. Avaa ensin nimetyt before-kuvat; ei rakenneuudistusta.
- **13:** MC Confirmation/Queue/Playlist/Pairing ListRow/filled-confirm/Cancel text/error; LG-dialogit vain teema, WallpaperDelete-confirm error. Testitagit ja callbackit ennallaan.
- **14:** formatter, assembleDebug, assembleDebugAndroidTest, full unit, lintDebug, detekt, ktlint; turvalliset fyysiset render-/UI-testit; asenna samaan sovellukseen, varmista signer/UID/paritukset; koko Pixel dark/light-matriisi1.15 ja saavutettavuus1.4/<300dp; vertaa kaikki after/before-kuvat, korjaa P0/P1; design-qa.md ja insinöörinäyttö/Roboto-lähdetiedot ajan tasalle.
- Executing-plans-skill vaatii lopuksi yhden tuoreen koko haaran review-agentin (ei tehtäväkohtaisia agenteja), yhden Critical/Important-korjauskierroksen RED→GREEN ja koko testisarjan. Arkistoi kaikki Ruling-/deferred minor -rivit ennen W:n poistamista; lopuksi paikallinen integrointi. Älä julkaise/pushaa.

## Komennot

Työhakemisto aina Androidin toteutustyöpuu. W/gradle.sh asettaa JDK25 launcher/testit, Gradle JDK17, SDK37 ja yhteisen cachen. Käytä alla olevaa JVM-valintaa. Lokin nimi vain `/tmp/remote-[A-Za-z0-9-]+.log`.

```bash
bash .superpowers/sdd/2026-11-16-remote-visual-redesign/gradle.sh --log /tmp/remote-redesign-t11-resume.log -Dorg.gradle.java.home=/home/emma/Dev/Mediacenter/.tools/remote-android/jdk17 :app:testDebugUnitTest --tests '*RemoteScreenTest' --tests '*ExistingTvRegressionTest' --tests '*TvAppsScreenTest' --tests '*TvAppsTransportTest' --tests '*PhoneUsabilityTest' :app:assembleDebug :app:detekt :app:ktlintCheck
```

Formatter on oma edeltävä komento `:app:ktlintFormat`; älä muokkaa lähteitä compiloinnin aikana. Git-commit käyttää komennonkohtaisesti `-c user.name=Codex -c user.email=codex@openai.com` (globaalia identiteettiä ei muutettu). Skill-skriptit `/home/emma/.codex/plugins/cache/openai-curated-remote/superpowers/7.0.1/skills/executing-plans/scripts/{task-start,task-done}`. Tehtävää11 ei aloiteta uudelleen uudella BASElla.

## Puhelin, viimeisin asennus ja turvallinen jatko

- Pixel9, sarjanumero46190DLAQ001B3,1080×2424,420dpi, font1.15. Käyttäjä kytki USB:n uudelleen; voi nyt irrottaa/sulkea koneen.
- **Puhelimessa valmis tehtävä10-versio dd1d77c**, APK SHA256 `620973bef7fb63fa2dc246b28a98655b07f1d56dbb6eab9d231bbe6ba97a30ca`. Keskeneräistä tehtävä11-versiota EI asennettu.
- Sama signer `c9091d20e12b6eeb96ef57e01ee1e27e57cf20fb17583f0dc1f803b8a1481046`, UID10450; neljä paritus-/asetustiedostoa tavuittain samat ennen käynnistystä. Todiste `installed/task10-library-install.json`.
- Puhelin jäi yhdistetyn Mediacenterin Library Photos -sivulle. Tuoreet `installed/task10-library-videos.png` ja `task10-library-photos.png` on otettu mutta **NIITÄ EI VIELÄ AVATTU VISUAALISESTI**. Aloita puhelin-QA niistä. Tarkista myös tehtävä10:n Music.
- **Keskeytyksessä palautettu:** `stay_on_while_plugged_in=0` (alkuperäinen), fontScale edelleen1.15; todiste W/pause-phone.json. Älä muuta globaalia fonttia testeihin; käytä Compose-overridea.
- APK asennetaan W/device.py:n `install LABEL`-toiminnolla, joka tarkistaa paketin/signerin/UID:n ja neljän tiedoston hashit. Älä koskaan uninstall/clear alkuperäistä Remotea. Testipaketin voi poistaa lopussa.
- W/device.py `click`, `capture`, `summary` tarkistavat sovellusfokuksen ja tuoreen UI-puun. Älä ohita fokussuojaa tai ota kuvia muista sovelluksista. Sovelluksen käynnistyksen jälkeen anna fokuksen asettua (välitön click voi saada `mCurrentFocus=null`).
- Force-stop/asennuksen jälkeen MC:n Wi-Fi-valinta on nykyisessä toteutuksessa tilapäinen. Paritus säilyy. Valitse **Change device → Mediacenter → Change device → Mediacenter connection → Wi-Fi → Reconnect → Close**. Tämä palautti yhteyden joka kerta, ilman controller-/VPN-/verkkokoodin muutoksia. Älä paina Remove computer.
- Native backend `/usr/bin/mediacenter` kuunteli192.168.1.101:47831; systemd-unit ei ole toiminnan lähde. Älä käynnistä prosesseja summassa tai muuta verkkoa/VPN:ää.
- ADB `/home/emma/Dev/Mediacenter/.tools/remote-android/sdk/platform-tools/adb`.
- Fyysisiin testeihin valitse vain turvalliset UI-luokat: tarkista ensin ettei testissä korruptoida alkuperäisen sovelluksen credentialeja tai tehdä oikeita TV/backend-komentoja. Tavallinen koko connected-testisetti voi sisältää siihen sopimattomia testejä. DesignCaptureTest ja IconRenderingTest ovat testidatalla toimivia Android-only render-testejä; laajenna MC-matriisiin turvallisesti tarvittaessa.
- Ei testiääniä, oikeaa playbackia, TV-kanavan/virran vaihtoa tai VPN-asetusmuutoksia tässä ulkoasutyössä. TV:n voimakkuus voi olla korkea. HDMI-peilaus ja ääni vahvistettiin aiemmin, ei tarvitse uutta testiä.
-117/118 kuvan alkuperäisiä tiedostoja ei löytynyt tallennetuista paikoista jo ennen tätä työtä. Säilytä lyhyet fallback-nimet; älä poista/reskannaa kirjastoa visuaalisen tehtävän vuoksi. Native1198media/4sources ja vanhan käyttöönoton avoimet Chrome-/Task21/22-rajat eivät muutu.
