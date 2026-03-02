use std::collections::HashMap;

use bincode::serialize;
use secs::{Entity, World};
use wrym::{Reliability, server::Server, server::Transport};

use dyrah_shared::{
    NetId,
    components::{Creature, Health},
    messages::{DamageEntry, ServerMessage},
    spells,
};

use crate::components::{CastCooldown, TilePos};

pub fn handle_cast(
    caster_id: NetId,
    spell_name: &str,
    world: &mut World,
    lobby: &HashMap<NetId, (Entity, String)>,
    server: &mut Server<Transport>,
) {
    let Some(spell) = spells::get(spell_name) else {
        return;
    };
    let Some(&(caster, _)) = lobby.get(&caster_id) else {
        return;
    };

    if let Some(mut cooldown) = world.get_mut::<CastCooldown>(caster) {
        if cooldown.remaining > 0.0 {
            return;
        }
        cooldown.remaining = spell.cooldown;
    }

    let origin = world.get::<TilePos>(caster).unwrap().vec;
    let affected_tiles = spells::area(origin, spell.range);

    let mut hit_entities: Vec<u64> = Vec::new();
    let mut to_damage: Vec<Entity> = Vec::new();

    world.query(|entity: Entity, _: &Creature, tile_pos: &TilePos| {
        if affected_tiles.contains(&tile_pos.vec) {
            hit_entities.push(entity.id());
            to_damage.push(entity);
        }
    });

    let mut damage_entries = Vec::new();
    let mut died_ids = Vec::new();
    for entity in to_damage {
        if let Some(mut health) = world.get_mut::<Health>(entity) {
            let damage = spell.damage.min(health.current);
            health.current -= damage;

            if health.current <= 0.0 {
                health.current = 0.0;
                world.despawn(Entity::from(entity.id()));
                died_ids.push(entity.id());
            }

            damage_entries.push(DamageEntry {
                id: entity.id(),
                damage,
                current: health.current,
                max: health.max,
            });
        }
    }

    if !damage_entries.is_empty() {
        server.broadcast(
            &serialize(&ServerMessage::EntitiesDamaged {
                entries: damage_entries,
            })
            .unwrap(),
            Reliability::ReliableOrdered { channel: 0 },
        );
    }

    if !died_ids.is_empty() {
        server.broadcast(
            &serialize(&ServerMessage::EntitiesDied { ids: died_ids }).unwrap(),
            Reliability::ReliableOrdered { channel: 0 },
        );
    }

    let msg = ServerMessage::SpellCast {
        caster_id: caster_id as u64,
        spell: spell_name.to_string(),
        origin,
        affected_tiles,
        hit_entities,
    };
    server.broadcast(
        &serialize(&msg).unwrap(),
        Reliability::ReliableOrdered { channel: 0 },
    );
}
