use std::collections::HashMap;

use egor::render::Graphics;

#[macro_export]
macro_rules! asset {
    ($name:literal) => {
        include_bytes!(concat!("../../assets/", $name))
    };
}

pub struct TextureManager {
    textures: HashMap<&'static str, usize>,
}

impl TextureManager {
    pub fn new() -> Self {
        Self {
            textures: HashMap::new(),
        }
    }

    pub fn load(&mut self, gfx: &mut Graphics, name: &'static str, bytes: &'static [u8]) {
        let id = gfx.load_texture(bytes);
        self.textures.insert(name, id);
    }

    pub fn get(&self, name: &str) -> usize {
        *self
            .textures
            .get(name)
            .unwrap_or_else(|| panic!("texture not found: {}", name))
    }
}
