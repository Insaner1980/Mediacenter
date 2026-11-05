use crate::contracts::{ServiceDefinition, ServiceId};
pub fn entries() -> Vec<ServiceDefinition> {
    use ServiceId::*;
    [
        (Netflix, "Netflix", "netflix.svg", "#080808"),
        (Youtube, "YouTube", "youtube.png", "#f90018"),
        (HboMax, "HBO Max", "hbo-max.png", "#46018a"),
        (Viaplay, "Viaplay", "viaplay.svg", "#600c41"),
        (MtvKatsomo, "MTV Katsomo", "mtv-katsomo.svg", "#4e0710"),
        (Ruutu, "Ruutu", "ruutu.svg", "#063f40"),
        (YleAreena, "Yle Areena", "yle-areena.svg", "#00796f"),
        (Plex, "Plex", "plex.svg", "#151515"),
    ]
    .into_iter()
    .map(|(id, name, logo_asset, tile_color)| ServiceDefinition {
        id,
        name: name.into(),
        logo_asset: logo_asset.into(),
        tile_color: tile_color.into(),
        vpn_required: true,
    })
    .collect()
}
pub fn key(id: ServiceId) -> &'static str {
    use ServiceId::*;
    match id {
        Netflix => "netflix",
        Youtube => "youtube",
        HboMax => "hbo_max",
        Viaplay => "viaplay",
        MtvKatsomo => "mtv_katsomo",
        Ruutu => "ruutu",
        YleAreena => "yle_areena",
        Plex => "plex",
    }
}
pub fn url(id: ServiceId) -> &'static str {
    use ServiceId::*;
    match id {
        Netflix => "https://www.netflix.com/",
        Youtube => "https://www.youtube.com/",
        HboMax => "https://www.hbomax.com/",
        Viaplay => "https://viaplay.fi/",
        MtvKatsomo => "https://www.mtv.fi/",
        Ruutu => "https://www.ruutu.fi/",
        YleAreena => "https://areena.yle.fi/",
        Plex => "https://app.plex.tv/",
    }
}
