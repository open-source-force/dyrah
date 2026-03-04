use std::collections::HashMap;

use dyrah_shared::map::{TiledMap, TiledTileset};
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
    pub current_z: i16,
}

impl Map {
    pub fn new(path: &str) -> Self {
        let mut tiled = TiledMap::new(path);
        tiled.tilesets.sort_by_key(|t| t.firstgid);

        Self {
            tiled,
            sets: HashMap::new(),
            current_z: 0,
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
            }
        }
    }

    fn find_tileset(&self, tile_id: u32) -> Option<&TiledTileset> {
        self.tiled
            .tilesets
            .iter()
            .rev()
            .find(|set| tile_id >= set.firstgid)
    }

    fn draw_tile_layer(&self, gfx: &mut Graphics, layer_name: &str) {
        let Some(layer) = self.tiled.get_layer(layer_name) else {
            return;
        };
        let Some(data) = &layer.data else { return };

        let layer_w = layer.width.unwrap();
        let layer_h = layer.height.unwrap();
        let tile_w = self.tiled.tilewidth;
        let tile_h = self.tiled.tileheight;

        for y in 0..layer_h {
            for x in 0..layer_w {
                let tile_id = data[(y * layer_w + x) as usize];
                if tile_id == 0 {
                    continue;
                }
                // strip flip/rotate flags from the high bits
                let tile_id = tile_id & 0x1FFFFFFF;

                let Some(tileset) = self.find_tileset(tile_id) else {
                    continue;
                };
                let Some(set) = self.sets.get(&tileset.firstgid) else {
                    continue;
                };

                let tex = set.texture;
                let (img_w, img_h) = set.dimensions;

                let ts_tile_w = tileset.tilewidth.unwrap();
                let ts_tile_h = tileset.tileheight.unwrap();
                let tiles_per_row = img_w / ts_tile_w;

                let local_id = tile_id - tileset.firstgid;
                let tile_x = (local_id % tiles_per_row) * ts_tile_w;
                let tile_y = (local_id / tiles_per_row) * ts_tile_h;

                let u0 = tile_x as f32 / img_w as f32;
                let v0 = tile_y as f32 / img_h as f32;
                let u1 = (tile_x + ts_tile_w) as f32 / img_w as f32;
                let v1 = (tile_y + ts_tile_h) as f32 / img_h as f32;

                let (offset_x, offset_y) = tileset
                    .tileoffset
                    .as_ref()
                    .map(|o| (o.x as f32, o.y as f32))
                    .unwrap_or((0.0, 0.0));

                let draw_x = (x * tile_w) as f32 + offset_x;
                let draw_y = (y * tile_h) as f32 - ts_tile_h as f32 + tile_h as f32 + offset_y;

                gfx.rect()
                    .at(Vec2::new(draw_x, draw_y))
                    .size(Vec2::new(ts_tile_w as f32, ts_tile_h as f32))
                    .texture(tex)
                    .uv([u0, v0, u1, v1]);
            }
        }
    }

    fn layer_z(name: &str) -> Option<i16> {
        name.split_once('/')
            .and_then(|(z, _)| z.parse::<i16>().ok())
    }

    fn get_roof_layer(&self, player_tile: IVec2) -> Option<i16> {
        self.tiled
            .layers
            .iter()
            .filter_map(|l| {
                l.name
                    .split_once("/roof")
                    .and_then(|(z, _)| z.parse::<i16>().ok())
            })
            .find(|&z| {
                let layer = format!("{}/roof", z);

                (-1..=1)
                    .flat_map(|dx| (-1..=1).map(move |dy| IVec2::new(dx, dy)))
                    .any(|offset| self.tiled.has_tile(&layer, player_tile + offset))
            })
    }

    pub fn draw_tiles(&self, gfx: &mut Graphics, player_tile: IVec2) {
        let inside = self.current_z < 0 || self.get_roof_layer(player_tile).is_some();

        for layer in &self.tiled.layers {
            if !layer.visible || layer.data.is_none() {
                continue;
            }

            let Some(z) = Self::layer_z(&layer.name) else {
                continue;
            };

            let should_draw = if inside {
                z == self.current_z
            } else {
                z >= self.current_z
            };

            if should_draw {
                self.draw_tile_layer(gfx, &layer.name);
            }
        }
    }
}
