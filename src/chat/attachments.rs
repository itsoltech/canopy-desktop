//! Images attached to chat messages: scaled to what the model reads, kept in the chat's
//! private attachment folder, encoded for sending. Other files go as paths.
use super::transcript::Attachment;
use base64::Engine;
use image::{DynamicImage, ImageFormat, imageops::FilterType};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Images per message.
pub const MAX_IMAGES: usize = 10;
/// The model reads images up to about this long edge; larger ones are scaled down.
const LONG_EDGE: u32 = 1568;
/// Encoded size limit before base64 (the API accepts 5 MB of base64 per image).
const MAX_BYTES: usize = 3_750_000;
/// Files larger than this are not read as images.
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;

/// Extensions treated as images when a file is attached.
const IMAGE_EXTENSIONS: [&str; 10] = [
    "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif",
];

pub fn is_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Reads an image file to attach (size-limited).
pub fn read_image(path: &Path) -> Result<Vec<u8>, String> {
    let size = std::fs::metadata(path)
        .map_err(|error| error.to_string())?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err("The image is too large.".into());
    }
    std::fs::read(path).map_err(|error| error.to_string())
}

/// Turns image bytes into an attachment: scaled and re-encoded when needed, saved in
/// `dir` (created private), with its data ready to send.
pub fn prepare_image(bytes: &[u8], dir: &Path) -> Result<Attachment, String> {
    let format = image::guess_format(bytes).map_err(|_| unsupported())?;
    let image = image::load_from_memory_with_format(bytes, format).map_err(|_| unsupported())?;
    let (width, height) = (image.width(), image.height());
    let kept = matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Gif
    ) && width.max(height) <= LONG_EDGE
        && bytes.len() <= MAX_BYTES;
    let (encoded, format, width, height) = if kept {
        (bytes.to_vec(), format, width, height)
    } else {
        let image = scale(image);
        let (width, height) = (image.width(), image.height());
        // Photos stay JPEG; screenshots and drawings keep exact pixels as PNG.
        let photo = matches!(format, ImageFormat::Jpeg | ImageFormat::WebP);
        let mut encoded = if photo { None } else { Some(png(&image)?) };
        if encoded.as_ref().is_none_or(|png| png.len() > MAX_BYTES) {
            encoded = None;
        }
        match encoded {
            Some(png) => (png, ImageFormat::Png, width, height),
            None => (jpeg(&image)?, ImageFormat::Jpeg, width, height),
        }
    };
    if encoded.len() > MAX_BYTES {
        return Err("The image is too large to send.".into());
    }
    let extension = format.extensions_str().first().copied().unwrap_or("img");
    let file = format!("{}.{extension}", uuid::Uuid::new_v4());
    write_private(&dir.join(&file), &encoded, dir)?;
    Ok(Attachment::Image {
        file,
        media_type: format.to_mime_type().to_owned(),
        width,
        height,
        data: Some(encode(&encoded)),
        label: None,
    })
}

/// The bytes of a saved image (for a preview).
pub fn image_bytes(dir: &Path, attachment: &Attachment) -> Option<Vec<u8>> {
    match attachment {
        Attachment::Image { file, .. } => std::fs::read(dir.join(safe_name(file)?)).ok(),
        _ => None,
    }
}

/// Loads the data of saved images that don't hold it (a message sent again after a
/// restart). Fails when an image is gone.
pub fn with_data(dir: &Path, attachments: Vec<Attachment>) -> Result<Vec<Attachment>, String> {
    attachments
        .into_iter()
        .map(|attachment| match attachment {
            Attachment::Image {
                file,
                media_type,
                width,
                height,
                data: None,
                label,
            } => {
                let bytes = safe_name(&file)
                    .and_then(|name| std::fs::read(dir.join(name)).ok())
                    .ok_or("An attached image is no longer available.")?;
                Ok(Attachment::Image {
                    file,
                    media_type,
                    width,
                    height,
                    data: Some(encode(&bytes)),
                    label,
                })
            }
            other => Ok(other),
        })
        .collect()
}

pub fn needs_data(attachments: &[Attachment]) -> bool {
    attachments
        .iter()
        .any(|a| matches!(a, Attachment::Image { data: None, .. }))
}

/// An attachment file name: a plain name inside the folder.
fn safe_name(file: &str) -> Option<&str> {
    let path = Path::new(file);
    (path.components().count() == 1 && path.file_name().is_some()).then_some(file)
}

fn unsupported() -> String {
    "This image format isn't supported (use PNG, JPEG, WebP or GIF).".into()
}

fn scale(image: DynamicImage) -> DynamicImage {
    if image.width().max(image.height()) <= LONG_EDGE {
        return image;
    }
    image.resize(LONG_EDGE, LONG_EDGE, FilterType::Lanczos3)
}

fn png(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(vec![]);
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(out.into_inner())
}

fn jpeg(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let mut out = vec![];
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    DynamicImage::ImageRgb8(image.to_rgb8())
        .write_with_encoder(encoder)
        .map_err(|error| error.to_string())?;
    Ok(out)
}

fn encode(bytes: &[u8]) -> Arc<str> {
    base64::engine::general_purpose::STANDARD
        .encode(bytes)
        .into()
}

fn write_private(path: &PathBuf, bytes: &[u8], dir: &Path) -> Result<(), String> {
    crate::platform::directories::ensure_private_dir(dir)?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn encoded(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut out = std::io::Cursor::new(vec![]);
        image.write_to(&mut out, format).expect("encode");
        out.into_inner()
    }

    #[test]
    fn small_images_are_kept_and_large_ones_scaled() {
        let dir = tempfile::tempdir().expect("dir");
        let small = encoded(
            DynamicImage::ImageRgb8(RgbImage::from_pixel(8, 4, Rgb([200, 10, 10]))),
            ImageFormat::Png,
        );
        let Attachment::Image {
            file,
            media_type,
            width,
            data,
            ..
        } = prepare_image(&small, dir.path()).expect("small")
        else {
            panic!("image")
        };
        assert_eq!((media_type.as_str(), width), ("image/png", 8));
        assert_eq!(std::fs::read(dir.path().join(&file)).expect("saved"), small);
        assert_eq!(&*data.expect("data"), encode(&small).as_ref());

        let large = encoded(
            DynamicImage::ImageRgb8(RgbImage::from_pixel(3000, 1000, Rgb([1, 2, 3]))),
            ImageFormat::Bmp,
        );
        let Attachment::Image {
            width,
            height,
            media_type,
            ..
        } = prepare_image(&large, dir.path()).expect("large")
        else {
            panic!("image")
        };
        assert_eq!((width, height), (LONG_EDGE, 523));
        assert_eq!(media_type, "image/png");
        assert!(prepare_image(b"not an image", dir.path()).is_err());
    }

    #[test]
    fn saved_images_load_their_data_again() {
        let dir = tempfile::tempdir().expect("dir");
        let bytes = encoded(
            DynamicImage::ImageRgb8(RgbImage::from_pixel(2, 2, Rgb([0, 0, 0]))),
            ImageFormat::Png,
        );
        let Attachment::Image {
            file,
            media_type,
            width,
            height,
            data,
            ..
        } = prepare_image(&bytes, dir.path()).expect("image")
        else {
            panic!("image")
        };
        let saved = vec![
            Attachment::Image {
                file: file.clone(),
                media_type,
                width,
                height,
                data: None,
                label: None,
            },
            Attachment::Path {
                path: "/tmp/x".into(),
                label: None,
            },
        ];
        assert!(needs_data(&saved));
        let loaded = with_data(dir.path(), saved).expect("loaded");
        assert!(!needs_data(&loaded));
        assert!(matches!(&loaded[0], Attachment::Image { data: d, .. } if *d == data));
        let gone = vec![Attachment::Image {
            file: "../escape.png".into(),
            media_type: "image/png".into(),
            width: 1,
            height: 1,
            data: None,
            label: None,
        }];
        assert!(with_data(dir.path(), gone).is_err());
        assert!(is_image_path(Path::new("/a/Shot.PNG")) && !is_image_path(Path::new("/a/b.txt")));
    }
}
