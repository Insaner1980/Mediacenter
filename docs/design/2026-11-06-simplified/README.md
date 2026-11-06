# Mediacenter — käyttöliittymäluonnokset 6.11.2026

Avaa index.html selaimessa tai PNG-kuvat kuvankatselimessa. Kuvat on tehty sisäänrakennetulla Image Gen -työkalulla käyttäjän kuvakaappauksista ja nykyisen projektin väreistä. Tarkat kehotteet ovat prompts.md-tiedostossa. Sovelluskoodia ei muutettu.

Kyse on saman suunnan kuudesta näkymästä. Musiikki-, video- ja kuvasisältö sekä toistoajat ovat havainnollistavaa esimerkkidataa. Kuvageneraattorin logo- ja typografiatulkinnat eivät korvaa projektin oikeita logoja ja fontteja toteutuksessa. Lisäkuvien kehotteet ovat prompts-additional.md-tiedostossa.

## Lisätyt näkymät

- 04-videos.png: videot esikatselukuvina, kesken jääneen videon Resume ja Play from start erillisinä valintoina. Haku, lähdesuodatus ja päivitys työkalurivillä, kansioiden toiminnot Manage folders -näkymässä. Videon avaaminen säilyttää esikatselun ja toiston nykyiset toiminnot.
- 05-photos.png: kuvien ruudukko on pääsisältö. Slideshow avaa näytön, aikavälin ja satunnaistoiston valinnat sekä käynnistyksen. Kuvan avaaminen säilyttää katselun, sovituksen ja todellisen koon; aktiivisen diaesityksen toiminnot näkyvät toiston aikana.
- 06-drives-usb.png: laitteet lyhyinä riveinä; osiot, tilat ja liittäminen avattavissa. Details säilyttää polut, osioiden koot ja laitetunnisteet. Manage media folders kokoaa nykyiset kirjastokansiotoiminnot. Safely remove avaa edelleen vahvistuksen koko fyysisen laitteen irrotuksesta ja kertoo siihen liittyvien aktiivisten toimintojen pysäyttämisestä. Estot ja virheet näytetään irrotuksen yhteydessä.

## Toiminnot säilyttävä rakenne

| Nykyinen toiminto | Ehdotettu paikka |
|---|---|
| Kahdeksan suoratoistopalvelua | Etusivun kahdeksan logoruutua |
| Videot, musiikki ja kuvat | Omat kirjastonäkymät, pysyvä sivuvalikko sisäsivuilla |
| Haku, lähteen valinta, päivitys ja sivutus | Kirjaston työkalurivi; sivutus vain tarvittaessa |
| Kansioiden lisäys, skannaus, peruutus ja poisto kirjastosta | Manage folders -näkymä; peruutus skannauksen aikana. Poisto kertoo lyhyesti, että tiedostot säilyvät |
| Musiikin jono, järjestäminen, kappaleen poisto ja jonon päivitys | Queue-välilehti; toiminnot valitulle riville |
| Soittolistojen luonti, tallennus, muokkaus, poisto ja jonon lisääminen | Playlists-välilehti ja erillinen editori; tallentamattomat muutokset näkyvät |
| Toisto, tauko, edellinen, seuraava, satunnaistoisto ja toisto uudelleen | Alareunan soitin; muut nykyiset soittimen toiminnot avattavissa |
| Kuvien katselu, sovitus ja todellinen koko | Kuvan avaaminen katselunäkymään |
| Diaesityksen aloitus, näyttö, aikaväli ja satunnaisuus | Photos-näkymän Slideshow-painikkeesta lyhyt asetusnäkymä |
| Diaesityksen tauko, edellinen, seuraava, lopetus ja paluu tietokoneelle | Aktiivisen diaesityksen ohjauspalkki |
| Levyjen päivitys, liittäminen ja turvallinen irrotus | Drives & USB; lyhyt laiterivi, osiot ja polut avattavissa |
| Ohjausnäytön valinta | Settings / Picture & sound / Controls; valinnan tallennus säilyy |
| Toiston kuva ja ääni | Settings / Picture & sound / Playback ja Sound; Save target tallentaa kohteen |
| Käynnissä olevan toiston siirto ja palautus tietokoneelle | Toiston aikana erilliset Move playback ja Bring back -toiminnot; kohteen tallennus ei yksin siirrä toistoa |
| Näyttöjen ja ääniulostulojen päivitys, vahvistetut ulostulot | Connection details; häiriöt näytetään suoraan olennaisen valinnan yhteydessä |
| VPN-maa, yhdistäminen, katkaisu ja tilan päivitys | VPN-osio; Connect tai Disconnect tilanteen mukaan |
| Kill switch ja palvelukohtainen VPN-vaatimus | VPN-osio ja avattava VPN by service -lista |
| Vaalea ja tumma teema | Appearance; luonnokset esittävät vaalean teeman |
| Diagnostiikan esikatselu ja vienti | Diagnostics |
| Palveluikkunan fokusointi, sulkeminen ja sovelluksesta poistuminen | Nykyiset istunto- ja poistumistoiminnot säilytetään; eivät esillä näissä kuvissa |

## Toteutuksessa täsmennettävää

Kuvat eivät esitä kaikkia avattuja paneeleja tai virhetilanteita. Kirjaston tyhjään tilaan riittää yksi Add folder -toiminto. Virheet, käynnissä oleva skannaus, VPN-vaatimukset ja tallentamattomat muutokset on edelleen näytettävä silloin kun ne vaikuttavat käyttäjän toimintaan. Harvoin tarvittava tekninen tieto siirtyy lisätietoihin.

Musiikkikuvan Settings-linkki näkyy kahdesti; toteutuksessa sivuvalikon linkki riittää sisäsivuilla. Sivupalkin ikonien käsittely tulee yhtenäistää kuvien välillä. Asetuskuvan Controls-valinnan tallennus on toteutuksessa nimettävä yksiselitteisesti erilleen toiston kohteesta tai Save target -painike nimettävä kattavasti Save changes. Kuvien pelkistys ei tarkoita taustatoimintojen poistamista.

## Toteutus ja varmennus 6.11.2026

Tämän luonnoskokonaisuuden toteutus on kirjattu [hyväksyntä- ja testiraporttiin](../../engineering/evidence/simplified-ui/acceptance.md). [Vertailugalleria](../../engineering/evidence/simplified-ui/index.html) sisältää alkuperäisen ja toteutuksen rinnakkain sekä päällekkäiskuvat, eri ikkunakoot, teemat ja natiivisovelluksen tarkistukset. [Visuaaliset poikkeamat ja päätökset](../../engineering/evidence/simplified-ui/visual-review.md) on kirjattu erikseen. Alkuperäisiä PNG-kuvia ei muutettu.
