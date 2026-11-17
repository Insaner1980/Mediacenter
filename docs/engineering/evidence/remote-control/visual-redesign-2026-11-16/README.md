# Remoten ulkoasu Pixel 9:llä

Kuvat on otettu alkuperäisessä Remotessa 17.11.2026. Kummassakin kuvassa LG on vasemmalla ja Mediacenter oikealla. Laitteen tekstikoko oli 1,15. Puhelimen järjestelmäpalkit on rajattu kuvista pois.

- [Tumma teema](target-controls-dark.png)
- [Vaalea teema](target-controls-light.png)

## Vertailu suunnitelman kuviin

| Näkymä | Pixelillä tarkistettu |
| --- | --- |
| LG Remote | Kolmirivinen reunustettu laitevalitsin, reunustetut näppäimet ja oranssi alavalikon valinta säilyvät. Ulkoasupainikkeessa on kontrastikuvake ja Color Dock -pikapainikkeessa avaamiskuvake. Neljä keskimmäistä näppäintä ovat yhtä korkeita. VOL ja CH näkyvät molemmissa teemoissa. Vieritetyssä sovellusrivissä näkyvät logot ja oikean reunan häivytys. |
| LG:n valitsimet ja välilehdet | Laitevalitsin, TV-yhteys ja ulkoasu avautuvat erillisiin ikkunoihin. Wallpapers ja Color Dock käyttävät yhteistä typografiaa ja värejä. |
| Mediacenter Remote | Yksirivinen laitevalitsin ja jonokuvake ovat otsikossa. Ohjaimen vinosaumat ovat poissa, Back ja Menu ovat vierekkäin sen alla ja palvelurivin logot häivyttyvät oikeaan reunaan. Alavalikko näyttää valitun välilehden. |
| Mediacenter Settings ja VPN | Pääsivulla on kuusi riviä. Kaikki alasivut avautuvat omilla otsikoillaan. VPN:n vierityksessä otsikko pysyy paikallaan ja palvelut näkyvät kuvakkeineen tiiviinä riveinä. |
| Mediacenter Library | Haku on lajivalintojen yläpuolella. Videot, musiikki ja kuvat latautuivat molemmissa teemoissa. Kuvat ovat kolmen sarakkeen ruudukossa. Korvaavan otsikon ja ruudun reunan väli näkyy myös vieritettäessä. Sivun lopussa on nimetty Next-painike. |
| Mediacenterin laitevalitsin | Valitsin avautuu alalehtenä. Valittu laite erottuu ja yhteysasetukset avautuvat omalta riviltään. LG-yhteyden toiminnot löytyvät LG:n yhteysikkunasta. |

Tumman teeman ensimmäinen musiikkihaku näytti **Try again**. Kirjaston päivityspainike latasi listan. Vaalea teema latasi saman listan ensimmäisellä yrityksellä.

Kuvakirjaston ennestään puuttuvilla alkuperäistiedostoilla näkyy korvaava kuvake ja otsikko. Kaksi vaalealla taustalla hillittyä LG-järjestelmälogoa käyttää TV:n toimittamaa alkuperäistä kuvapintaa. LG:n sovellusrivin alareunan ja navigoinnin väli näkyy kokonaan pienen pystysuuntaisen vierityksen jälkeen.

## Tarkistustulokset

486 yksikkö- ja Compose-testiä, 57 Pixel-testiä, molemmat APK-koonnit, Android lint, Detekt ja ktlint läpäisivät. Pixelin valmiin Wallpapers- ja Color Dock -tilan näkymät tarkistettiin testiaineistolla molemmissa teemoissa. Käyttöliittymän testit kattoivat myös 1,4-kertaisen tekstin ja alle 300 dp leveän ikkunan. Remoten allekirjoitus, UID sekä molemmat paritus- ja asetustiedostoparit täsmäsivät päivityksessä.

Kuvavertailussa ja riippumattomassa koodikatselmoinnissa ei löytynyt toiminnon estäviä tai merkittäviä uusia ulkoasuongelmia. Katselmoinnin vähäiset huomiot koskivat käytöstä poistettujen listarivien värejä ja vierityksen aikaista uudelleenpiirtoa. Yksityiset vertailukuvat, testiloki ja asennustodisteet ovat kansiossa `.superpowers/sdd/2026-11-16-remote-redesign/installed/`.
