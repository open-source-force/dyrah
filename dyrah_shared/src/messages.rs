use glam::{IVec2, Vec2};
use serde::{Deserialize, Serialize};

use crate::NetId;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatureSpawn {
    pub kind: String,
    pub position: Vec2,
    pub health: f32,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatureMove {
    pub id: u64,
    pub position: Vec2,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DamageEntry {
    pub id: u64,
    pub damage: f32,
    pub current: f32,
    pub max: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
    AuthSuccess {
        id: NetId,
        username: String,
        password: String,
    },
    AuthFailed {
        reason: String,
    },
    PlayerSpawned {
        id: NetId,
        username: String,
        position: Vec2,
        health: f32,
    },
    PlayerDespawned {
        id: NetId,
    },
    PlayerMoved {
        id: NetId,
        position: Vec2,
        path: Option<Vec<Vec2>>,
    },
    CreatureBatchSpawned(Vec<CreatureSpawn>),
    CreatureBatchMoved(Vec<CreatureMove>),
    EntitiesDamaged {
        entries: Vec<DamageEntry>,
    },
    EntitiesDied {
        ids: Vec<u64>,
    },
    ChatReceived {
        sender_id: NetId,
        text: String,
    },
    SpellCast {
        caster_id: u64,
        spell: String,
        origin: IVec2,
        affected_tiles: Vec<IVec2>,
        hit_entities: Vec<u64>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Login { username: String, password: String },
    Register { username: String, password: String },
    PlayerUpdate { input: ClientInput },
    ChatMessage { text: String },
    CastSpell { spell: String },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClientInput {
    pub left: bool,
    pub up: bool,
    pub right: bool,
    pub down: bool,
    pub mouse_tile_pos: Option<IVec2>,
}

impl ClientInput {
    pub fn to_direction(&self) -> IVec2 {
        IVec2::new(
            (self.right as i32) - (self.left as i32),
            (self.down as i32) - (self.up as i32),
        )
    }
}
