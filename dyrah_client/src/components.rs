use egor::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::sprite::Animation;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WorldPos {
    pub vec: Vec2,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TargetWorldPos {
    pub vec: Vec2,
    pub path: Option<Vec<Vec2>>,
}

#[derive(Debug)]
pub struct Sprite {
    pub anim: Animation,
}
