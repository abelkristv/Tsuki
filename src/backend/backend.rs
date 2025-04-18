use std::any::Any;

use smithay::{backend::renderer::{element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer}, desktop::space::SpaceRenderElements};

use crate::Tsuki;

pub trait Backend: Any {
    fn seat_name(&self) -> String;
    fn renderer(&mut self) -> Option<&mut GlesRenderer>;
    fn render(
        &mut self,
        tsuki: &mut Tsuki,
        elements: &[SpaceRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    );
    fn init(&mut self, tsuki: &mut Tsuki); 
    fn as_any (&mut self) -> &mut dyn Any;
}