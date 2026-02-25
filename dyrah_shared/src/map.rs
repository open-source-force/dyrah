use std::fs::read_to_string;

use glam::{IVec2, Vec2};
use serde::Deserialize;
use serde_json::from_str;

#[derive(Deserialize, Debug)]
pub struct TileOffset {
    pub x: i32,
    pub y: i32,
}

#[derive(Deserialize, Debug)]
pub struct TiledObject {
    pub id: u32,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Deserialize, Debug)]
pub struct TiledLayer {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub name: String,
    pub visible: bool,
    pub data: Option<Vec<u32>>,
    pub objects: Option<Vec<TiledObject>>,
}

#[derive(Deserialize, Debug)]
pub struct TiledTileset {
    pub firstgid: u32,
    pub source: Option<String>,
    pub image: Option<String>,
    pub tilecount: Option<u32>,
    pub tilewidth: Option<u32>,
    pub tileheight: Option<u32>,
    pub tileoffset: Option<TileOffset>,
}

#[derive(Deserialize)]
pub struct TiledMap {
    pub width: u32,
    pub height: u32,
    pub tilewidth: u32,
    pub tileheight: u32,
    pub layers: Vec<TiledLayer>,
    pub tilesets: Vec<TiledTileset>,
}

impl TiledMap {
    pub fn new(path: &str) -> Self {
        let content = read_to_string(path).expect("Failed to read map file");
        from_str(&content).expect("Failed to parse JSON map")
    }

    pub fn tile_size(&self) -> IVec2 {
        IVec2::new(self.tilewidth as i32, self.tileheight as i32)
    }

    pub fn get_layer(&self, layer_name: &str) -> Option<&TiledLayer> {
        self.layers.iter().find(|l| l.name == layer_name)
    }

    pub fn get_object(&self, layer_name: &str, name: &str) -> Option<&TiledObject> {
        self.get_layer(layer_name)
            .and_then(|l| l.objects.as_ref()?.iter().find(|o| o.name == name))
    }

    pub fn is_walkable(&self, layer_name: &str, tile_pos: IVec2) -> bool {
        let layer = match self.get_layer(layer_name) {
            Some(l) => l,
            None => return true,
        };

        let tile_x = tile_pos.x as usize;
        let tile_y = tile_pos.y as usize;
        if tile_x >= layer.width.unwrap() as usize || tile_y >= layer.height.unwrap() as usize {
            return false; // out of bounds = blocked
        }

        let index = tile_y * layer.width.unwrap() as usize + tile_x;
        layer
            .data
            .as_ref()
            .and_then(|data| data.get(index))
            .map_or(false, |&tile| tile == 0)
    }

    pub fn tile_to_world(&self, tile_pos: IVec2) -> Vec2 {
        Vec2::new(
            tile_pos.x as f32 * self.tilewidth as f32,
            tile_pos.y as f32 * self.tileheight as f32,
        )
    }

    pub fn world_to_tile(&self, world_pos: Vec2) -> IVec2 {
        IVec2::new(
            (world_pos.x / self.tilewidth as f32).floor() as i32,
            (world_pos.y / self.tileheight as f32).floor() as i32,
        )
    }

    pub fn get_tile(&self, layer_name: &str, world_pos: Vec2) -> Option<IVec2> {
        let layer = self.get_layer(layer_name)?;
        let tile_pos = self.world_to_tile(world_pos);
        let index = (tile_pos.y * layer.width? as i32 + tile_pos.x) as usize;

        if layer
            .data
            .as_ref()
            .and_then(|data| data.get(index))
            .map_or(false, |&tile| tile != 0)
        {
            return Some(tile_pos);
        }

        None
    }
}
