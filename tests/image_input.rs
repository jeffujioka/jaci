use image::Rgba;
use jaci::image_input;

#[test]
fn supported_formats_load() {
    for extension in ["png", "jpg", "webp", "bmp", "gif", "tiff"] {
        let path = format!("test_fixtures/4x4.{extension}");

        let image = image_input::read(&path).unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(image.dimensions(), (4, 4), "{path}");
    }
}

#[test]
fn gif_first_frame() {
    let image = image_input::read("test_fixtures/two-frames.gif").unwrap();

    assert!(
        image.pixels().all(|pixel| *pixel == Rgba([255, 0, 0, 255])),
        "{:?}",
        image.pixels().collect::<Vec<_>>()
    );
}

#[test]
fn missing_image_reported() {
    let error = image_input::read("missing.png").unwrap_err();

    assert_eq!(error.to_string(), "jaci: image not found: missing.png");
}

#[test]
fn corrupt_image_reported() {
    let error = image_input::read("test_fixtures/corrupt.png").unwrap_err();

    assert_eq!(
        error.to_string(),
        "jaci: cannot decode image test_fixtures/corrupt.png: unsupported or corrupt file"
    );
}
