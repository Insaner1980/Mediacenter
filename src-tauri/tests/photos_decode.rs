use mediacenter_lib::photos::decode;
use std::{fs::File, io::Write};
use tokio_util::sync::CancellationToken;
fn image_file(bytes: &[u8]) -> tempfile::NamedTempFile {
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(bytes).unwrap();
    file
}
fn jpeg() -> Vec<u8> {
    let out = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=red:s=80x40",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "mjpeg",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}
#[tokio::test]
async fn raster_decoding_and_exif_orientation() {
    let bytes = jpeg();
    let plain = image_file(&bytes);
    let decoded = decode::decode_file(plain.as_file(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!((decoded.width, decoded.height), (80, 40));
    // JPEG APP1 containing a little-endian TIFF orientation=6 (90 degrees clockwise).
    let exif: Vec<u8> = vec![
        b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0, 0,
        0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut rotated = bytes[..2].to_vec();
    rotated.extend([255, 225]);
    rotated.extend(((exif.len() + 2) as u16).to_be_bytes());
    rotated.extend(exif);
    rotated.extend(&bytes[2..]);
    let file = image_file(&rotated);
    let decoded = decode::decode_file(file.as_file(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!((decoded.width, decoded.height), (40, 80));
}
#[tokio::test]
async fn unsafe_corrupt_and_oversize_files_are_rejected() {
    for bytes in [
        b"<html><script>alert(1)</script>".as_slice(),
        b"<svg xmlns='http://www.w3.org/2000/svg'/>",
        b"\xff\xd8corrupt",
    ] {
        let file = image_file(bytes);
        assert!(
            decode::decode_file(file.as_file(), CancellationToken::new())
                .await
                .is_err()
        );
    }
    let file = image_file(&jpeg());
    File::options()
        .write(true)
        .open(file.path())
        .unwrap()
        .set_len(70 * 1024 * 1024)
        .unwrap();
    assert!(
        decode::decode_file(file.as_file(), CancellationToken::new())
            .await
            .is_err()
    );
}
#[tokio::test]
async fn cancellation_prevents_image_delivery() {
    let file = image_file(&jpeg());
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(decode::decode_file(file.as_file(), cancel).await.is_err());
}
#[tokio::test]
async fn supported_avif_decodes_to_safe_jpeg() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("photo.avif");
    let out = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=64x32",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-c:v",
            "libaom-av1",
            "-cpu-used",
            "8",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "AVIF fixture encoder failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let file = File::open(path).unwrap();
    let image = decode::decode_file(&file, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!((image.width, image.height), (64, 32));
    assert_eq!(image.image.mime, "image/jpeg");
}
#[tokio::test]
async fn oversized_pixel_dimensions_are_rejected_before_frame_decoding() {
    let out = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=2x2",
            "-frames:v",
            "1",
            "-threads",
            "1",
            "-f",
            "image2pipe",
            "-c:v",
            "png",
            "pipe:1",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let mut bytes = out.stdout;
    bytes[16..20].copy_from_slice(&10000u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&10000u32.to_be_bytes());
    let mut crc = 0xffff_ffffu32;
    for value in &bytes[12..29] {
        crc ^= u32::from(*value);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xedb8_8320 } else { 0 };
        }
    }
    bytes[29..33].copy_from_slice(&(!crc).to_be_bytes());
    let file = image_file(&bytes);
    let error = decode::decode_file(file.as_file(), CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(error.code, "image_too_large");
}
