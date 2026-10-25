use iced::advanced::graphics::geometry::Renderer as GeometryRenderer;
use iced::advanced::renderer::Headless;
use iced::widget::canvas::Geometry;
use iced::{Color, Renderer, Size};

pub async fn renderer() -> Renderer {
    Renderer::new(iced::Font::DEFAULT, iced::Pixels(16.0), Some("tiny-skia"))
        .await
        .expect("headless canvas renderer")
}

pub fn render(renderer: &mut Renderer, geometry: Vec<Geometry>, size: Size<u32>) -> Vec<u8> {
    for geometry in geometry {
        renderer.draw_geometry(geometry);
    }
    renderer.screenshot(size, 1.0, Color::WHITE)
}

pub fn assert_region_eq(
    actual: &[u8],
    expected: &[u8],
    width: usize,
    region: [usize; 4],
    context: &str,
) {
    let [left, top, right, bottom] = region;
    for y in top..bottom {
        for x in left..right {
            let offset = (y * width + x) * 4;
            assert_eq!(
                &actual[offset..offset + 4],
                &expected[offset..offset + 4],
                "{context}: canvas pixel at ({x}, {y})"
            );
        }
    }
}
