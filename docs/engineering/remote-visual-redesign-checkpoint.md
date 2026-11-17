# Remoten ulkoasu-uudistuksen tarkistus

**Valmis 17.11.2026.** Suunnitelman tehtävät 1–14 on toteutettu, testattu ja yhdistetty Androidin alkuperäiseen `/home/emma/Dev/Remote`-työpuuhun. Paikallinen haara `codex/initial-import` osoittaa committiin `b84741a`.

- `b34e65f` viimeisteli LG Remote -näppäimet, VOL- ja CH-merkinnät, ulkoasupainikkeen ja sovellusrivin. Wallpapers ja Color Dock tarkistettiin molemmissa teemoissa.
- `975c48c` viimeisteli Mediacenterin vahvistukset, jonon, soittolistat ja paritusikkunan sekä LG:n kuvanpoiston vahvistustekstin.
- `900d278` lisäsi kapean näkymän ja uusien kuvakkeiden tarkistukset. `b84741a` tallensi Pixel-tulokset ja Roboto Flexin lähde- ja lisenssitiedot.

486 yksikkö- ja Compose-testiä, 57 Pixel-testiä, molemmat APK-koonnit, Android lint, Detekt ja ktlint läpäisivät. Pixelillä tarkistettiin 46 kuvaa ja testiaineistolla myös Wallpapers- ja Color Dock -välilehtien valmiit näkymät. Koko haaran riippumaton katselmointi löysi kaksi vähäistä huomiota. Käytöstä poistettujen listarivien värit näyttävät aktiivisilta ja Mediacenterin otsikko piirtyy tarpeettoman usein vierityksessä. [Kuvavertailu ja tarkistustulokset](evidence/remote-control/visual-redesign-2026-11-16/README.md)

## Puhelin

Pixel 9:ssä on alkuperäinen `com.emmah.colordock.remote`, nimi Remote, UID 10450 ja sama allekirjoitus. `975c48c`:sta kootun APK:n SHA-256 on `4f6d29ab13395d4342d7c702421b77f64c3025db904168eb89c276ed92482075`. Neljä paritus- ja asetustiedostoa täsmäsi tavuittain sekä päivityksessä että Pixel-testien yli. LG TV ja Mediacenter näkyivät lopuksi yhdistettyinä. Jono avattiin ja suljettiin alkuperäisessä sovelluksessa. Testipaketti poistettiin, ja puhelimeen jäi Mediacenterin Settings tummassa teemassa. Fonttikerroin on alkuperäinen `1.15` ja normaali näytön sammuminen on palautettu arvoon `0`.

Tumman teeman musiikkilista latautui yhden päivityksen jälkeen ja vaalean teeman lista ensimmäisellä kerralla. Aiemmin puuttuviksi todettujen valokuvien kohdalla näkyy korvaava kuvake ja otsikko.

## Aineisto

Yksityiset Pixel-kuvat, asennustodisteet ja testiloki ovat kansiossa `.superpowers/sdd/2026-11-16-remote-redesign/installed/`. Tehtäväkohtaiset päätökset, testit ja katselmointi on arkistoitu saman aineiston `implementation-ledger/`-kansioon. Androidin yksityiskohtainen tarkistus on tiedostossa `/home/emma/Dev/Remote/design-qa.md`.
