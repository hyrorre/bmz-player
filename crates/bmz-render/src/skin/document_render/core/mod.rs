mod clip;
mod resolve;
mod static_render;

pub(super) use clip::skin_document_render_core_clip_methods;
pub(super) use resolve::skin_document_render_core_resolve_methods;
pub(super) use static_render::{
    fixed_image_destination_cacheable, skin_document_render_core_static_methods,
    static_image_destination_cacheable,
};
