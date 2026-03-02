use crate::{
    components::{Sprite, TargetWorldPos, WorldPos},
    sprite::Direction,
};
use dyrah_shared::{
    TILE_SIZE,
    components::{Creature, Player},
};
use secs::World;

pub fn update(world: &mut World, dt: f32) {
    let move_speed = 3.0 * TILE_SIZE;

    world.query(
        |_, _: &Player, pos: &mut WorldPos, target_pos: &mut TargetWorldPos, spr: &mut Sprite| {
            interpolate(pos, target_pos, spr, move_speed, dt);
            if pos.vec == target_pos.vec {
                target_pos.path = None;
            }
        },
    );

    world.query(
        |_, _: &Creature, pos: &mut WorldPos, target_pos: &mut TargetWorldPos, spr: &mut Sprite| {
            interpolate(pos, target_pos, spr, move_speed, dt);
        },
    );
}

fn interpolate(
    pos: &mut WorldPos,
    target_pos: &mut TargetWorldPos,
    spr: &mut Sprite,
    move_speed: f32,
    dt: f32,
) {
    if pos.vec != target_pos.vec {
        let diff = target_pos.vec - pos.vec;
        let dist = diff.length();
        let step = move_speed * dt;
        pos.vec = if step >= dist {
            target_pos.vec
        } else {
            pos.vec + diff * (step / dist)
        };
        spr.anim.set_direction(direction_from_diff(diff));
        spr.anim.update(dt);
    } else {
        spr.anim.reset_to_idle();
    }
}

fn direction_from_diff(diff: egor::math::Vec2) -> Direction {
    if diff.x.abs() > diff.y.abs() {
        if diff.x > 0.0 {
            Direction::Right
        } else {
            Direction::Left
        }
    } else {
        if diff.y > 0.0 {
            Direction::Down
        } else {
            Direction::Up
        }
    }
}
