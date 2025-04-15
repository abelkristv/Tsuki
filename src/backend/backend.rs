use smithay::{backend::renderer::{element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer}, desktop::space::SpaceRenderElements};

use crate::Tsuki;

pub trait Backend {
    fn set_name(&self) -> String;
    fn renderer(&mut self) -> &mut GlesRenderer;
    fn render(
        &mut self,
        tsuki: &mut Tsuki,
        elements: &[SpaceRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    );
}