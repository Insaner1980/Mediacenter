use base64::Engine;
use mediacenter_lib::{app_state::AppState, library::thumbnails};
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::PathBuf,
};
use tokio_util::sync::CancellationToken;
struct Library {
    _directory: tempfile::TempDir,
    state: AppState,
    root: PathBuf,
}
impl Library {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("photos");
        std::fs::create_dir(&root).unwrap();
        let state = AppState::open(directory.path().join("app")).unwrap();
        state.db.lock().unwrap().execute("INSERT INTO media_sources(id,source_type,root,label,kinds,availability) VALUES('source','local',?,'Photos','[\"photo\"]','available')",[root.as_os_str().as_bytes()]).unwrap();
        Self {
            _directory: directory,
            state,
            root,
        }
    }
    fn fixture(&self, name: &str, codec: &str, color: &str) {
        let result = std::process::Command::new("/usr/bin/ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:s=1200x800"),
                "-frames:v",
                "1",
                "-threads",
                "1",
                "-f",
                "image2pipe",
                "-c:v",
                codec,
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        std::fs::write(self.root.join(name), result.stdout).unwrap();
        self.index(name);
    }
    fn index(&self, name: &str) {
        let metadata = std::fs::metadata(self.root.join(name)).unwrap();
        self.state.db.lock().unwrap().execute("INSERT INTO media_items(id,source_id,relative_path,display_name,kind,size,mtime_ns,availability) VALUES(?1,'source',?2,?1,'photo',?3,?4,'available') ON CONFLICT(id) DO UPDATE SET size=excluded.size,mtime_ns=excluded.mtime_ns",rusqlite::params![name,name.as_bytes(),metadata.len()as i64,format!("{}:{}",metadata.mtime(),metadata.mtime_nsec())]).unwrap();
    }
    async fn get(&self, name: &str) -> Result<Vec<u8>, mediacenter_lib::contracts::AppError> {
        let image = thumbnails::get(&self.state, name, CancellationToken::new()).await?;
        assert_eq!(image.mime, "image/png");
        Ok(base64::engine::general_purpose::STANDARD
            .decode(image.base64)
            .unwrap())
    }
    fn cache_path(&self, name: &str) -> PathBuf {
        let filename:String=self.state.db.lock().unwrap().query_row("SELECT filename FROM thumbnail_entries WHERE media_id=? ORDER BY last_access_ms DESC LIMIT 1",[name],|r|r.get(0)).unwrap();
        self.state.cache_dir.join("thumbnails").join(filename)
    }
}
#[tokio::test]
async fn thumbnails_real_jpeg_and_png_decode_to_bounded_pngs() {
    let library = Library::new();
    for (name, codec) in [("photo.jpg", "mjpeg"), ("photo.png", "png")] {
        library.fixture(name, codec, "red");
        let image = library.get(name).await.unwrap();
        assert!(thumbnails::valid_png(&image));
        assert_eq!(u32::from_be_bytes(image[16..20].try_into().unwrap()), 640);
        assert_eq!(u32::from_be_bytes(image[20..24].try_into().unwrap()), 427);
        assert_eq!(library.get(name).await.unwrap(), image);
    }
}
#[tokio::test]
async fn thumbnails_reject_corrupt_and_oversize_photos_without_caching() {
    let library = Library::new();
    for (name, bytes) in [
        ("broken.jpg", b"\xff\xd8\xffbroken".as_slice()),
        (
            "document.png",
            b"<svg><script>alert(1)</script></svg>".as_slice(),
        ),
    ] {
        std::fs::write(library.root.join(name), bytes).unwrap();
        library.index(name);
        assert!(library.get(name).await.is_err());
    }
    library.fixture("large.jpg", "mjpeg", "red");
    std::fs::OpenOptions::new()
        .write(true)
        .open(library.root.join("large.jpg"))
        .unwrap()
        .set_len(65 * 1024 * 1024)
        .unwrap();
    library.index("large.jpg");
    assert!(library.get("large.jpg").await.is_err());
    let count: i64 = library
        .state
        .db
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM thumbnail_entries", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
#[tokio::test]
async fn thumbnails_regenerate_a_truncated_cached_png() {
    let library = Library::new();
    library.fixture("photo.jpg", "mjpeg", "red");
    let original = library.get("photo.jpg").await.unwrap();
    let mut corrupt = original.clone();
    corrupt[40] ^= 1;
    for invalid in [original[..24].to_vec(), corrupt] {
        std::fs::write(library.cache_path("photo.jpg"), &invalid).unwrap();
        let regenerated = library.get("photo.jpg").await.unwrap();
        assert!(
            regenerated == original,
            "corrupt cached image was returned instead of regenerated"
        );
    }
}
#[tokio::test]
async fn thumbnails_cache_hits_restore_missing_accounting_and_remove_orphans() {
    let library = Library::new();
    library.fixture("photo.jpg", "mjpeg", "red");
    let original = library.get("photo.jpg").await.unwrap();
    let path = library.cache_path("photo.jpg");
    library
        .state
        .db
        .lock()
        .unwrap()
        .execute("DELETE FROM thumbnail_entries", [])
        .unwrap();
    let orphan = path
        .parent()
        .unwrap()
        .join(format!("{}.png", "e".repeat(64)));
    std::fs::write(&orphan, b"old orphan").unwrap();
    let unrelated = path.parent().unwrap().join("notes.txt");
    std::fs::write(&unrelated, b"keep").unwrap();
    assert_eq!(library.get("photo.jpg").await.unwrap(), original);
    let bytes: i64 = library
        .state
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT COALESCE(SUM(bytes),0) FROM thumbnail_entries",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(bytes, original.len() as i64);
    assert!(!orphan.exists());
    assert!(unrelated.exists());
}
#[tokio::test]
async fn thumbnails_source_changes_never_return_the_previous_image() {
    let library = Library::new();
    library.fixture("photo.jpg", "mjpeg", "red");
    let original = library.get("photo.jpg").await.unwrap();
    let old_path = library.cache_path("photo.jpg");
    library.fixture("replacement.jpg", "mjpeg", "blue");
    std::fs::copy(
        library.root.join("replacement.jpg"),
        library.root.join("photo.jpg"),
    )
    .unwrap();
    assert_eq!(
        library.get("photo.jpg").await.unwrap_err().code,
        "media_changed"
    );
    library.index("photo.jpg");
    let fresh = library.get("photo.jpg").await.unwrap();
    assert_ne!(fresh, original);
    assert_ne!(library.cache_path("photo.jpg"), old_path);
}
#[tokio::test]
async fn thumbnails_lru_budget_evicts_oldest_files_and_keeps_recent_images() {
    let library = Library::new();
    library.fixture("photo.jpg", "mjpeg", "red");
    let directory = library.state.cache_dir.join("thumbnails");
    std::fs::create_dir_all(&directory).unwrap();
    for index in 1..=257 {
        let key = format!("{index:064x}");
        let filename = format!("{key}.png");
        std::fs::File::create(directory.join(&filename))
            .unwrap()
            .set_len(2 * 1024 * 1024)
            .unwrap();
        library.state.db.lock().unwrap().execute("INSERT INTO thumbnail_entries(cache_key,media_id,fingerprint,filename,bytes,last_access_ms) VALUES(?1,'photo.jpg',?1,?2,2097152,?3)",rusqlite::params![key,filename,index]).unwrap();
    }
    let image = library.get("photo.jpg").await.unwrap();
    assert!(thumbnails::valid_png(&image));
    let bytes: i64 = library
        .state
        .db
        .lock()
        .unwrap()
        .query_row("SELECT SUM(bytes) FROM thumbnail_entries", [], |r| r.get(0))
        .unwrap();
    assert!(bytes <= 512 * 1024 * 1024);
    assert!(!directory.join(format!("{:064x}.png", 1)).exists());
    assert!(!directory.join(format!("{:064x}.png", 2)).exists());
    assert!(directory.join(format!("{:064x}.png", 257)).exists());
    assert!(library.cache_path("photo.jpg").exists());
}
