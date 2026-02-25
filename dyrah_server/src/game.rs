use std::{collections::HashMap, path::Path, time::Duration};

use bincode::{deserialize, serialize};
use secs::{Entity, World};
use wrym::{
    Reliability,
    server::{Server, ServerConfig, ServerEvent, Transport},
};

use dyrah_shared::{
    NetId,
    components::Player,
    messages::{ClientMessage, ServerMessage},
};

use crate::{
    components::{Collider, TargetTilePos, TilePos},
    db::Database,
    map::{CollisionGrid, Map},
};

pub struct Game {
    db: Database,
    server: Server<Transport>,
    pending: HashMap<NetId, String>,
    lobby: HashMap<NetId, (Entity, String)>,
    world: World,
    collision_grid: CollisionGrid,
    map: Map,
}

impl Game {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let map = Map::new("assets/map.json");

        Self {
            db: Database::new(path),
            server: Server::new(
                Transport::new("0.0.0.0:8080"),
                ServerConfig {
                    client_timeout: Duration::from_mins(10),
                },
            ),
            pending: HashMap::new(),
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
                    let addr = self.server.client_addr(id).unwrap().clone();
                    println!("Client {} connected from {}", id, addr);
                    // park them in pending until they authenticate
                    self.pending.insert(id, addr);
                }
                ServerEvent::ClientDisconnected(id) => {
                    println!("Client {} disconnected.", id);

                    self.pending.remove(&id);
                    if let Some((player, _)) = self.lobby.remove(&id) {
                        self.world.despawn(player);
                    }

                    let msg = ServerMessage::PlayerDespawned { id };
                    self.server.broadcast(
                        &serialize(&msg).unwrap(),
                        Reliability::ReliableOrdered { channel: 0 },
                    );
                }
                ServerEvent::MessageReceived(id, bytes) => match deserialize(&bytes).unwrap() {
                    ClientMessage::Register { username, password } => {
                        match self.db.register(&username, &password) {
                            Ok(true) => {
                                println!("New player registered: {}", username);
                                if let Some(addr) = self.pending.remove(&id) {
                                    self.spawn_player(id, username, &addr);
                                }
                            }
                            _ => {
                                if let Some(addr) = self.pending.get(&id) {
                                    let msg = ServerMessage::AuthFailed {
                                        reason: "Username already taken".into(),
                                    };
                                    self.server.send_to(
                                        addr,
                                        &serialize(&msg).unwrap(),
                                        Reliability::ReliableOrdered { channel: 0 },
                                    );
                                }
                            }
                        }
                    }
                    ClientMessage::Login { username, password } => {
                        match self.db.login(&username, &password) {
                            Ok(true) => {
                                println!("Player logged in: {}", username);
                                if let Some(addr) = self.pending.remove(&id) {
                                    self.spawn_player(id, username, &addr);
                                }
                            }
                            _ => {
                                if let Some(addr) = self.pending.get(&id) {
                                    let msg = ServerMessage::AuthFailed {
                                        reason: "Invalid username or password".into(),
                                    };
                                    self.server.send_to(
                                        addr,
                                        &serialize(&msg).unwrap(),
                                        Reliability::ReliableOrdered { channel: 0 },
                                    );
                                }
                            }
                        }
                    }
                    ClientMessage::ChatMessage { text } => {
                        if self.lobby.contains_key(&id) {
                            let msg = ServerMessage::ChatReceived {
                                sender_id: id,
                                text,
                            };
                            self.server
                                .broadcast(&serialize(&msg).unwrap(), Reliability::Unreliable);
                        }
                    }
                    ClientMessage::PlayerUpdate { input } => {
                        if let Some(&(player, _)) = self.lobby.get(&id) {
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
                                self.server
                                    .broadcast(&serialize(&msg).unwrap(), Reliability::Unreliable);
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

    fn spawn_player(&mut self, id: NetId, username: String, addr: &String) {
        // sync existing players to the new client
        for (&other_id, &(player, ref other_name)) in &self.lobby {
            let target_pos = self.world.get::<TargetTilePos>(player).unwrap();
            let msg = ServerMessage::PlayerSpawned {
                id: other_id,
                username: other_name.clone(),
                position: self.map.tiled.tile_to_world(target_pos.vec),
            };
            self.server.send_to(
                addr,
                &serialize(&msg).unwrap(),
                Reliability::ReliableOrdered { channel: 0 },
            );
        }

        let spawn_pos = self.map.get_spawn("player").unwrap();
        let player = self.world.spawn((
            Player,
            TilePos { vec: spawn_pos },
            TargetTilePos { vec: spawn_pos },
            Collider,
        ));
        self.lobby.insert(id, (player, username.clone()));

        let msg = ServerMessage::PlayerSpawned {
            id,
            username,
            position: self.map.tiled.tile_to_world(spawn_pos),
        };
        self.server.broadcast(
            &serialize(&msg).unwrap(),
            Reliability::ReliableOrdered { channel: 0 },
        );
    }
}
