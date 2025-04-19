use std::any::Any;

use smithay::{backend::renderer::{element::surface::WaylandSurfaceRenderElement, gles::GlesRenderer}, desktop::space::SpaceRenderElements};

use crate::{state::OutputRenderElements, Tsuki};

pub trait Backend: Any {
    fn seat_name(&self) -> String;
    fn renderer(&mut self) -> Option<&mut GlesRenderer>;
    fn render(
        &mut self,
        tsuki: &mut Tsuki,
        elements: &[OutputRenderElements<GlesRenderer,
            WaylandSurfaceRenderElement<GlesRenderer>>]
    );
    fn init(&mut self, tsuki: &mut Tsuki); 
    fn as_any (&mut self) -> &mut dyn Any;
}