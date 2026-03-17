pub mod background;
pub mod bake;
pub mod display;
pub mod geometry;
pub mod post;

pub const INTERMEDIATE_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba32Float;
pub const POST_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg11b10Ufloat;
pub const BACKGROUND_TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg11b10Ufloat;
