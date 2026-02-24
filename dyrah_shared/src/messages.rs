use glam::{IVec2, Vec2};
use serde::{Deserialize, Serialize};

use crate::NetId;

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
    },
    PlayerDespawned {
        id: NetId,
    },
    PlayerMoved {
        id: NetId,
        position: Vec2,
    },
    ChatReceived {
        sender_id: NetId,
        text: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Login { username: String, password: String },
    Register { username: String, password: String },
    PlayerUpdate { input: ClientInput },
    ChatMessage { text: String },
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
