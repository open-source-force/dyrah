use glam::IVec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TilePos {
    pub vec: IVec2,
    pub z: i16,
}

impl From<IVec2> for TilePos {
    fn from(value: IVec2) -> Self {
        Self { vec: value, z: 0 }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TargetTilePos {
    pub vec: IVec2,
    pub path: Option<Vec<IVec2>>,
    pub delay: f32,
}

pub struct Collider;
pub struct CastCooldown {
    pub remaining: f32,
}
