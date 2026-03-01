use std::collections::HashMap;

use dyrah_shared::map::TiledMap;
use egor::{
    math::{IVec2, Vec2},
    render::Graphics,
};

pub struct Tileset {
    dimensions: (u32, u32),
    texture: usize,
}

pub struct Map {
    pub tiled: TiledMap,
    sets: HashMap<u32, Tileset>,
    pub current_level: usize,
}

impl Map {
    pub fn new(path: &str) -> Self {
        let tiled = TiledMap::new(path);
        Self {
            tiled,
            sets: HashMap::new(),
            current_level: 0,
        }
    }

    pub fn load(&mut self, gfx: &mut Graphics) {
        for tileset in &self.tiled.tilesets {
            if let Some(image_path) = &tileset.image {
                let filename = std::path::Path::new(image_path).file_name().unwrap();
                let bytes = std::fs::read(std::path::Path::new("assets").join(filename)).unwrap();
                let img = image::load_from_memory(&bytes).unwrap().to_rgba8();
                let (w, h) = img.dimensions();
                let tex = gfx.load_texture(&bytes);

                self.sets.insert(
                    tileset.firstgid,
                    Tileset {
                        dimensions: (w, h),
                        texture: tex,
                    },
                );

                println!("Loaded tileset: {} ({}x{})", image_path, w, h);
            }
        }
    }

    pub fn draw_tile_layer(&self, gfx: &mut Graphics, layer_name: &str) {
        let layer = self.tiled.get_layer(layer_name).unwrap();
        let (layer_w, layer_h) = (layer.width.unwrap(), layer.height.unwrap());
        let (tile_w, tile_h) = (self.tiled.tilewidth, self.tiled.tileheight);

        for y in 0..layer_h {
            for x in 0..layer_w {
                if let Some(data) = &layer.data {
                    let tile_id = data[(y * layer_w + x) as usize];
                    if tile_id == 0 {
                        continue;
                    }

                    if let Some(tileset) = self
                        .tiled
                        .tilesets
                        .iter()
                        .filter(|set| tile_id >= set.firstgid)
                        .last()
                    {
                        let tex = self.sets[&tileset.firstgid].texture;
                        let (img_w, img_h) = self.sets[&tileset.firstgid].dimensions;
                        let tileset_tile_w = tileset.tilewidth.unwrap();
                        let tileset_tile_h = tileset.tileheight.unwrap();
                        let tiles_per_row = img_w / tileset_tile_w;
                        let local_tile_id = tile_id - tileset.firstgid;
                        let (tile_x, tile_y) = (
                            local_tile_id % tiles_per_row * tileset_tile_w,
                            local_tile_id / tiles_per_row * tileset_tile_h,
                        );

                        let u0 = tile_x as f32 / img_w as f32;
                        let v0 = tile_y as f32 / img_h as f32;
                        let u1 = (tile_x + tileset_tile_w) as f32 / img_w as f32;
                        let v1 = (tile_y + tileset_tile_h) as f32 / img_h as f32;

                        // add offset when tiles are larger
                        let offset_x = tileset.tileoffset.as_ref().map_or(0, |o| o.x) as f32;
                        let offset_y = tileset.tileoffset.as_ref().map_or(0, |o| o.y) as f32;
                        let draw_x = (x * tile_w) as f32 + offset_x;
                        // account for Tiled's Y-down & egor's Y-up
                        let draw_y =
                            (y * tile_h) as f32 - tileset_tile_h as f32 + tile_h as f32 + offset_y;

                        gfx.rect()
                            .at(Vec2::new(draw_x, draw_y))
                            .size(Vec2::new(tileset_tile_w as f32, tileset_tile_h as f32))
                            .texture(tex)
                            .uv([u0, v0, u1, v1]);
                    }
                }
            }
        }
    }

    fn get_roof_level(&self, player_tile: IVec2) -> Option<usize> {
        self.tiled
            .layers
            .iter()
            .filter_map(|l| {
                l.name
                    .strip_prefix("level_")
                    .and_then(|s| s.strip_suffix("/roof"))
                    .and_then(|n| n.parse::<usize>().ok())
            })
            .find(|&level| {
                let layer = format!("level_{}/roof", level);
                (-1..=1_i32)
                    .flat_map(|dx| (-1..=1_i32).map(move |dy| IVec2::new(dx, dy)))
                    .any(|offset| self.tiled.has_tile(&layer, player_tile + offset))
            })
    }

    pub fn draw_tiles(&self, gfx: &mut Graphics, player_tile: IVec2) {
        if self.get_roof_level(player_tile).is_some() {
            let ground = format!("level_{}/ground", self.current_level);
            let walls = format!("level_{}/walls", self.current_level);
            if self.tiled.get_layer(&ground).is_some() {
                self.draw_tile_layer(gfx, &ground);
            }
            if self.tiled.get_layer(&walls).is_some() {
                self.draw_tile_layer(gfx, &walls);
            }
        } else {
            for layer in &self.tiled.layers {
                if layer.visible && layer.data.is_some() {
                    self.draw_tile_layer(gfx, &layer.name);
                }
            }
        }
    }
}
