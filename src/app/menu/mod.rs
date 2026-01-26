use std::sync::Arc;

use glam::UVec2;
use gpu_layout::{AsGpuBytes, GpuBytes};

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

        let mut array = [UVec2::ZERO; 128];

        for (i, &e) in self.characters.iter().enumerate() {
            array[i] = e;
        }

        data.write(&self.line_index)
            .write(&(self.characters.len() as u32))
            .write(&array);

        data
    }
}

pub struct Menu {
    font: Arc<Font>,
    lines: Vec<TextLine>,
}

impl Menu {
    pub fn new(font: Arc<Font>) -> Self {
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

pub struct Menus {
    font: Arc<Font>,
    pub left: Menu,
    pub right: Menu,
}

impl Menus {
    pub fn new() -> Self {
        let font = Arc::new(Font::new());

        Self {
            left: Menu::new(font.clone()),
            right: Menu::new(font.clone()),
            font,
        }
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }
}
