pub mod background;
pub mod bake;
pub mod display;
pub mod geometry;
pub mod post;

/*
** passes **

compile-time:
    - bake 0...n
        - atmosphere LUTs

runtime:
    - background
        - atmosphere or cubemap
    - geometry
        - path trace, debug render, or BVH debug render
    - post 0...n
        - bloom, tone map, exposure, gamma correct, menu
    - display (is render pass, writes to surface texture, simple blit?)
*/

pub const INTERMEDIATE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
pub const POST_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg11b10Ufloat;
