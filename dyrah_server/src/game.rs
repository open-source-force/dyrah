use std::collections::HashMap;

use bincode::{deserialize, serialize};
use secs::{Entity, World};
use wrym::{
    server::{Server, ServerConfig, ServerEvent},
    transport::Transport,
};

use dyrah_shared::{
    NetId,
    components::Player,
    messages::{ClientMessage, ServerMessage},
};

use crate::{
    components::{Collider, TargetTilePos, TilePos},
    map::{CollisionGrid, Map},
};

pub struct Game {
    server: Server<Transport>,
    lobby: HashMap<NetId, Entity>,
    world: World,
    collision_grid: CollisionGrid,
    map: Map,
}

impl Game {
    pub fn new() -> Self {
        let map = Map::new("assets/map.json");

        Self {
            server: Server::new(Transport::new("127.0.0.1:8080"), ServerConfig::default()),
            lobby: HashMap::new(),
            world: World::default(),
            collision_grid: CollisionGrid::new(&map),
            map,
        }
    }

    pub fn handle_events(&mut self) {
        self.server.poll();
        while let Some(event) = self.server.recv_event() {
            match event {
                ServerEvent::ClientConnected(id) => {
                    let addr = self.server.client_addr(id).unwrap();
                    println!("Client {} connected from {}", id, addr);

                    // sync existing players on new clients
                    for (&other_id, &player) in &self.lobby {
                        let target_pos = self.world.get::<TargetTilePos>(player).unwrap();
                        let msg = ServerMessage::PlayerSpawned {
                            id: other_id,
                            position: self.map.tiled.tile_to_world(target_pos.vec),
                        };

                        self.server
                            .send_reliable_to(&addr, &serialize(&msg).unwrap(), None);
                    }

                    let spawn_pos = self.map.get_spawn("player").unwrap();
                    let player = self.world.spawn((
                        Player,
                        TilePos { vec: spawn_pos },
                        TargetTilePos { vec: spawn_pos },
                        Collider,
                    ));
                    self.lobby.insert(id, player);

                    println!(
                        "Spawned player {} at tile: {:?}, world: {:?}",
                        id,
                        spawn_pos,
                        self.map.tiled.tile_to_world(spawn_pos)
                    );

                    let msg = ServerMessage::PlayerSpawned {
                        id,
                        position: self.map.tiled.tile_to_world(spawn_pos),
                    };
                    self.server
                        .broadcast_reliable(&serialize(&msg).unwrap(), None);
                }
                ServerEvent::ClientDisconnected(id) => {
                    println!("Client {} disconnected.", id);

                    self.lobby.remove(&id).map(|p| self.world.despawn(p));

                    let msg = ServerMessage::PlayerDespawned { id };
                    self.server
                        .broadcast_reliable(&serialize(&msg).unwrap(), None);
                }
                ServerEvent::MessageReceived(id, bytes) => match deserialize(&bytes).unwrap() {
                    ClientMessage::ChatMessage { text } => {
                        let msg = ServerMessage::ChatReceived {
                            sender_id: id,
                            text,
                        };
                        self.server
                            .broadcast_reliable(&serialize(&msg).unwrap(), None);
                    }
                    ClientMessage::PlayerUpdate { input } => {
                        if let Some(&player) = self.lobby.get(&id) {
                            let mut target_pos =
                                self.world.get_mut::<TargetTilePos>(player).unwrap();
                            let mut tile_pos = self.world.get_mut::<TilePos>(player).unwrap();

                            let next_pos = target_pos.vec + input.to_direction();

                            if self.map.is_walkable(next_pos, &self.collision_grid) {
                                target_pos.vec = next_pos;
                                tile_pos.vec = next_pos;

                                let msg = ServerMessage::PlayerMoved {
                                    id,
                                    position: self.map.tiled.tile_to_world(tile_pos.vec),
                                };
                                self.server.broadcast(&serialize(&msg).unwrap());
                            }
                        }
                    }
                },
            }
        }
    }

    pub fn update(&mut self, _dt: f32) {
        self.collision_grid.update(&self.map, &self.world);
    }
}
