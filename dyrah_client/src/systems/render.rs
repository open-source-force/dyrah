use std::collections::HashMap;

use egor::{
    math::{IVec2, Vec2},
    render::{Color, Graphics},
};
use secs::{Entity, World};

use dyrah_shared::{
    TILE_SIZE,
    components::{Creature, Player},
};

use crate::{
    components::{Sprite, TargetWorldPos, WorldPos},
    map::Map,
    texture::TextureManager,
};

pub fn creatures(world: &World, gfx: &mut Graphics, textures: &TextureManager) {
    world.query(
        |_, creature: &Creature, world_pos: &WorldPos, spr: &Sprite| {
            gfx.rect()
                .at(world_pos.vec - TILE_SIZE / 4.0)
                .size(Vec2::splat(TILE_SIZE))
                .texture(textures.get(&creature.kind))
                .uv(spr.anim.frame());
        },
    );
}

pub fn players(
    world: &World,
    gfx: &mut Graphics,
    textures: &TextureManager,
    local_player: Option<Entity>,
) -> Vec2 {
    let mut player_world_pos = Vec2::ZERO;
    world.query(|entity, _: &Player, world_pos: &WorldPos, spr: &Sprite| {
        gfx.rect()
            .at(world_pos.vec - TILE_SIZE / 4.0)
            .size(Vec2::splat(TILE_SIZE))
            .texture(textures.get("player"))
            .uv(spr.anim.frame());
        if Some(entity) == local_player {
            player_world_pos = world_pos.vec;
        }
    });
    player_world_pos
}

pub fn chat_bubbles(
    world: &World,
    gfx: &mut Graphics,
    latest_msgs: &HashMap<Entity, (String, f32)>,
    player_world_pos: Vec2,
    screen: Vec2,
) {
    world.query(|entity, _: &Player, world_pos: &WorldPos, _: &Sprite| {
        if let Some((msg, age)) = latest_msgs.get(&entity) {
            let alpha = if *age > 8.0 {
                1.0 - (*age - 8.0) / 2.0
            } else {
                1.0
            };
            let screen_pos = world_pos.vec - player_world_pos + screen / 2.0;
            gfx.text(msg)
                .at((screen_pos.x, screen_pos.y - 10.0))
                .color(Color::new([1.0, 1.0, 1.0, alpha]));
        }
    });
}

pub fn path(gfx: &mut Graphics, points: &[Vec2]) {
    if points.len() < 2 {
        return;
    }
    let half = TILE_SIZE / 2.0;
    let centers: Vec<Vec2> = points.iter().map(|&p| p + Vec2::splat(half)).collect();
    gfx.polyline()
        .points(&centers)
        .thickness(1.0)
        .color(Color::new([0.0, 0.0, 0.0, 1.0]));

    for &point in points {
        let s = TILE_SIZE;
        gfx.polyline()
            .points(&[
                point,
                point + Vec2::new(s, 0.0),
                point + Vec2::new(s, s),
                point + Vec2::new(0.0, s),
            ])
            .closed(true)
            .thickness(1.5)
            .color(Color::new([0.5, 0.0, 1.0, 0.8]));
        gfx.polygon()
            .at(point + Vec2::splat(half))
            .radius(2.0)
            .segments(8)
            .color(Color::new([0.5, 0.0, 1.0, 1.0]));
    }
}

pub fn hover_tile(gfx: &mut Graphics, tile: IVec2, map: &Map) {
    let p = map.tiled.tile_to_world(tile);
    let s = TILE_SIZE;
    gfx.polyline()
        .points(&[
            p,
            p + Vec2::new(s, 0.0),
            p + Vec2::new(s, s),
            p + Vec2::new(0.0, s),
        ])
        .closed(true)
        .thickness(1.5)
        .color(Color::new([1.0, 0.5, 0.0, 1.0]));
}

pub fn debug(
    gfx: &mut Graphics,
    world: &World,
    local_player: Option<Entity>,
    map: &Map,
    hovered_tile: Option<IVec2>,
    fps: u32,
) {
    gfx.text(&format!("FPS: {}", fps)).at((10.0, 10.0));

    if let Some(player) = local_player {
        if let (Some(pos), Some(target)) = (
            world.get::<WorldPos>(player),
            world.get::<TargetWorldPos>(player),
        ) {
            let tile = map.tiled.world_to_tile(pos.vec);
            let target_tile = map.tiled.world_to_tile(target.vec);
            let hover = hovered_tile.unwrap_or(IVec2::ZERO);
            gfx.text(&format!("World: ({:.0}, {:.0})", pos.vec.x, pos.vec.y))
                .at((10.0, 30.0));
            gfx.text(&format!("Tile: ({}, {})", tile.x, tile.y))
                .at((10.0, 50.0));
            gfx.text(&format!(
                "Target tile: ({}, {})",
                target_tile.x, target_tile.y
            ))
            .at((10.0, 70.0));
            gfx.text(&format!("Hovered tile: ({}, {})", hover.x, hover.y))
                .at((10.0, 90.0));
        }
    }
}
