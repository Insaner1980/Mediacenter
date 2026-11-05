pub mod playlists;
pub mod queue;
pub mod service;
pub use service::on_end;
pub fn typescript(config: &ts_rs::Config) -> Vec<String> {
    use ts_rs::TS;
    vec![
        queue::MusicItem::decl(config),
        queue::QueueEntry::decl(config),
        queue::RepeatMode::decl(config),
        queue::MusicQueue::decl(config),
        queue::MusicAction::decl(config),
        playlists::PlaylistSummary::decl(config),
        playlists::Playlist::decl(config),
    ]
}
