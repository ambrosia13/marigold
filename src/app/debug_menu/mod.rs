use std::sync::Arc;

use bevy_ecs::{
    resource::Resource,
    system::{Commands, Res, ResMut},
};
use glam::UVec2;
use gpu_layout::{AsGpuBytes, GpuBytes, Std140Layout, Std430Layout};
use wgpu::util::DeviceExt;

use crate::app::render::SurfaceState;

const MAX_CHARS_PER_LINE: usize = 128;
const MAX_LINES: usize = 128;

pub struct Font {
    bitmap: [u64; 128],
    char_size: usize,
    char_padding: usize,
    line_size: usize,
    line_padding: usize,
}

impl Font {
    pub fn parse_bitmap(source: &str) -> anyhow::Result<[u64; 128]> {
        let mut bitmap = [0u64; 128];

        for (line_index, line) in source.trim().lines().enumerate() {
            let mut bytes = [0u8; 8];

            for (byte_index, byte) in line.trim().split_ascii_whitespace().enumerate() {
                let byte = u8::from_str_radix(byte, 16)?;
                bytes[byte_index] = byte;
            }

            bitmap[line_index] = u64::from_be_bytes(bytes);
        }

        Ok(bitmap)
    }

    pub fn new() -> Self {
        Self {
            bitmap: Self::parse_bitmap(include_str!("font.txt")).unwrap(),
            char_size: 8,
            char_padding: 1,
            line_size: 8,
            line_padding: 1,
        }
    }
}

#[derive(Clone)]
pub struct TextLine {
    line_index: u32,
    characters: Vec<UVec2>,
}

impl TextLine {
    pub fn new(line_index: u32) -> Self {
        Self {
            line_index,
            characters: Vec::new(),
        }
    }

    pub fn set_text(&mut self, font: &Font, text: &str) {
        self.characters.clear();
        self.characters = text
            .chars()
            .map(|c| c as u8)
            .map(|ascii| font.bitmap[(ascii % 128) as usize])
            .map(|bitmap| {
                UVec2::new(
                    (bitmap & ((1 << 32) - 1)) as u32,
                    (bitmap >> 32 & ((1 << 32) - 1)) as u32,
                )
            })
            .collect();
    }
}

impl AsGpuBytes for TextLine {
    fn as_gpu_bytes<L: gpu_layout::GpuLayout + ?Sized>(&self) -> gpu_layout::GpuBytes<'_, L> {
        let mut data = GpuBytes::empty();

        let mut array = [UVec2::ZERO; MAX_CHARS_PER_LINE];

        for (i, &e) in self.characters.iter().enumerate() {
            array[i] = e;
        }

        data.write(&self.line_index)
            .write(&(self.characters.len() as u32))
            .write(&array);

        data
    }
}

pub struct DebugMenu {
    font: Arc<Font>,
    lines: Vec<TextLine>,
}

impl DebugMenu {
    fn new(font: Arc<Font>) -> Self {
        Self {
            font,
            lines: Vec::new(),
        }
    }

    pub fn println(&mut self, text: &str) {
        let mut line = TextLine::new(self.lines.len() as u32);
        line.set_text(&self.font, text);

        self.lines.push(line);
    }

    pub fn reset(&mut self) {
        self.lines.clear();
    }
}

#[derive(Resource)]
pub struct DebugMenus {
    font: Arc<Font>,
    pub left: DebugMenu,
    pub right: DebugMenu,
    pub scale: f32,
}

impl DebugMenus {
    fn new() -> Self {
        let font = Arc::new(Font::new());

        let left = DebugMenu::new(font.clone());
        let right = DebugMenu::new(font.clone());

        Self {
            left,
            right,
            font,
            scale: 2.0,
        }
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    pub fn init(mut commands: Commands) {
        commands.insert_resource(Self::new());
        log::info!("initialized debug menus");
    }

    // clear menu every frame
    pub fn update(mut debug: ResMut<DebugMenus>) {
        debug.reset();

        debug
            .left
            .println("Rachel Amber mentioned!! Rachel isn't just a character.");
        debug
            .left
            .println("She's a phenomenon. She's a celestial event, a once-in-a-lifetime");
        debug
            .left
            .println("alignment of the stars, a cosmic masterpiece sculpted by the");
        debug
            .left
            .println("creators of Life is Strange themselves.");

        debug
            .left
            .println("Loving Rachel Amber isn't a hobby, it's a lifestyle,");
        debug.left.println("a reason to live.");

        debug
            .left
            .println("Every day I wake up and think of this woman.");
        debug
            .left
            .println("There isn't one day where this woman isn't on my mind.");

        debug
            .left
            .println("Rachel is what fuels my day; she is so gorgeous, beautiful,");
        debug
            .left
            .println("radiant, captivating, charming, elegant, striking,");
        debug
            .left
            .println("dashing, alluring, handsome, lovely, mesmerizing,");
        debug
            .left
            .println("enchanting, breathtaking, irresistible, charismatic,");
        debug
            .left
            .println("fashionable, incredible, incomparable, graceful,");
        debug
            .left
            .println("sophisticated, unforgettable, impressive, flawless,");
        debug
            .left
            .println("awe-inspiring, timeless, divine, jaw-dropping, dreamy,");
        debug
            .left
            .println("mesmeric, admirable, dazzling, impeccable, majestic,");
        debug
            .left
            .println("ethereal, unforgettable, incomparable, breathtaking,");
        debug
            .left
            .println("elegant, radiant, unrivaled, enchanting, alluring,");
        debug.left.println("pretty, graceful...");

        debug.left.println("Oh my Rachel. I love Rachel Amber.");
        debug.left.println("I'm her n1 fan.");

        debug
            .right
            .println("Because most LPR-centered systems use automatic");
        debug
            .right
            .println("video/picture scanning, infrared (IR) light detectors");
        debug
            .right
            .println("can be used to report when such an LPR scans something.");
        debug
            .right
            .println("With a small enough form-factor, these detectors can");
        debug
            .right
            .println("be mounted directly on a vehicle's license plate");
        debug
            .right
            .println("(front or rear) to react to the event of TAPS scanning it.");

        debug
            .right
            .println("Upon activation, these devices can directly or");
        debug
            .right
            .println("indirectly signal out to a remote centralized system");
        debug
            .right
            .println("that aggregates all data, builds an activity hotspot");
        debug
            .right
            .println("timeline, and exposes a breakdown of TAPS activity");
        debug.right.println("by region.");

        debug.right.println("Crowd Sourcing");

        debug
            .right
            .println("Alone, such a device cannot possibly gather enough");
        debug
            .right
            .println("data to build such a chart in a cheap manner.");
        debug
            .right
            .println("The solution to this? The good people of Switzerland!");
        debug
            .right
            .println("Through the power of distributed intelligence,");
        debug
            .right
            .println("these devices can build a huge amount of space");
        debug
            .right
            .println("and time coverage, increasing reliability");

        debug.right.println("Phone Integration");

        debug
            .right
            .println("Furthermore, the device can communicate strictly");
        debug
            .right
            .println("with the user's phone, which can be paired");
        debug
            .right
            .println("upon activation when it comes to transmission.");

        debug.right.println("This has the following advantages:");

        debug
            .right
            .println("Privacy data like GPS location is sent only");
        debug
            .right
            .println("with user permission, protecting them from");
        debug
            .right
            .println("being tracked by the server if a breach occurs.");

        debug
            .right
            .println("The user can attach extra metadata such as");
        debug
            .right
            .println("exact parking garage, floor, or section.");

        debug
            .right
            .println("When a device detects an LPR scan, it can");
        debug
            .right
            .println("broadcast a ping to nearby devices, allowing");
        debug
            .right
            .println("them to warn their users they may want");
        debug.right.println("to re-park soon!");
    }
}

#[derive(Resource)]
pub struct DebugMenuBinding {
    menu_data: wgpu::Buffer,
    left_menu: wgpu::Buffer,
    right_menu: wgpu::Buffer,

    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

impl DebugMenuBinding {
    fn prepare_menu_data(menus: &DebugMenus) -> GpuBytes<'_, Std140Layout> {
        let mut buf = GpuBytes::empty();

        // char padding
        buf.write(&0u32);

        // line padding
        buf.write(&1u32);

        // left line count
        buf.write(&(menus.left.lines.len() as u32));

        // right line count
        buf.write(&(menus.right.lines.len() as u32));

        // scale
        buf.write(&menus.scale);

        buf
    }

    fn prepare_menu(menu: &DebugMenu) -> GpuBytes<'_, Std430Layout> {
        let mut buf = GpuBytes::empty();

        let mut array = std::array::from_fn::<_, MAX_LINES, _>(|_| TextLine::new(0));

        for (i, l) in menu.lines.iter().enumerate().take(array.len()) {
            array[i] = l.clone();
        }

        buf.write(&array);

        buf
    }

    fn menu_element_size() -> usize {
        let line = TextLine::new(0);

        let mut buf = GpuBytes::<Std430Layout>::empty();
        buf.write(&line);

        buf.as_slice().len()
    }

    pub fn init(mut commands: Commands, surface_state: Res<SurfaceState>, menus: Res<DebugMenus>) {
        log::info!("beginning creation of debug menu binding");

        let gpu = &surface_state.gpu;

        let menu_data = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("debug_menu_data_buffer"),
            size: Self::prepare_menu_data(&menus).as_slice().len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let left_menu = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("left_debug_menu_buffer"),
            size: Self::prepare_menu(&menus.left)
                .as_slice()
                .len()
                .max(Self::menu_element_size()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let right_menu = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("right_debug_menu_buffer"),
            size: Self::prepare_menu(&menus.right)
                .as_slice()
                .len()
                .max(Self::menu_element_size()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("debug_menu_bind_group_layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("debug_menu_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: menu_data.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: left_menu.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: right_menu.as_entire_binding(),
                },
            ],
        });

        let menu_binding = Self {
            menu_data,
            left_menu,
            right_menu,
            bind_group,
            bind_group_layout,
        };

        commands.insert_resource(menu_binding);
        log::info!("created debug menu binding");
    }

    pub fn update(
        surface_state: Res<SurfaceState>,
        menus: Res<DebugMenus>,
        mut menu_binding: ResMut<DebugMenuBinding>,
    ) {
        // first, update menu metadata buffer
        surface_state.gpu.queue.write_buffer(
            &menu_binding.menu_data,
            0,
            Self::prepare_menu_data(&menus).as_slice(),
        );

        // second, update left & right array buffers
        let mut left_data = Self::prepare_menu(&menus.left);
        let mut right_data = Self::prepare_menu(&menus.right);

        let left_data = left_data.as_slice();
        let right_data = right_data.as_slice();

        let mut remake_bind_group = false;

        if menu_binding.left_menu.size() == left_data.len() as u64 {
            // write into existing buffer
            surface_state
                .gpu
                .queue
                .write_buffer(&menu_binding.left_menu, 0, left_data);
        } else {
            // remake buffer
            if !left_data.is_empty() {
                menu_binding.left_menu = surface_state.gpu.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("left_debug_menu_buffer"),
                        contents: left_data,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    },
                );
            } else {
                menu_binding.left_menu =
                    surface_state
                        .gpu
                        .device
                        .create_buffer(&wgpu::BufferDescriptor {
                            label: Some("left_debug_menu_buffer"),
                            size: Self::prepare_menu(&menus.left)
                                .as_slice()
                                .len()
                                .max(Self::menu_element_size())
                                as u64,
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
            }

            remake_bind_group = true;
        }

        if menu_binding.right_menu.size() == right_data.len() as u64 {
            // write into existing buffer
            surface_state
                .gpu
                .queue
                .write_buffer(&menu_binding.right_menu, 0, right_data);
        } else {
            // remake buffer
            if !right_data.is_empty() {
                menu_binding.right_menu = surface_state.gpu.device.create_buffer_init(
                    &wgpu::util::BufferInitDescriptor {
                        label: Some("right_debug_menu_buffer"),
                        contents: right_data,
                        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    },
                );
            } else {
                menu_binding.right_menu =
                    surface_state
                        .gpu
                        .device
                        .create_buffer(&wgpu::BufferDescriptor {
                            label: Some("right_debug_menu_buffer"),
                            size: Self::prepare_menu(&menus.right)
                                .as_slice()
                                .len()
                                .max(Self::menu_element_size())
                                as u64,
                            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
            }

            remake_bind_group = true;
        }

        if remake_bind_group {
            // remake bind group because at least one of the buffers was recreated
            menu_binding.bind_group =
                surface_state
                    .gpu
                    .device
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("debug_menu_bind_group"),
                        layout: &menu_binding.bind_group_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: menu_binding.menu_data.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: menu_binding.left_menu.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: menu_binding.right_menu.as_entire_binding(),
                            },
                        ],
                    });
        }
    }
}
