use std::{collections::HashMap, path::Path, time::Duration};

use bincode::{deserialize, serialize};
use glam::IVec2;
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

                            if let Some(mouse_tile_pos) = input.mouse_tile_pos {
                                if let Some(path) = self.map.find_path(
                                    tile_pos.vec,
                                    mouse_tile_pos,
                                    &self.collision_grid,
                                ) {
                                    let world_path: Vec<_> = path
                                        .iter()
                                        .map(|&t| self.map.tiled.tile_to_world(t))
                                        .collect();

                                    target_pos.path = Some(path);

                                    let msg = ServerMessage::PlayerMoved {
                                        id,
                                        position: self.map.tiled.tile_to_world(tile_pos.vec),
                                        path: Some(world_path),
                                    };
                                    self.server.broadcast(
                                        &serialize(&msg).unwrap(),
                                        Reliability::Unreliable,
                                    );
                                }
                            } else {
                                let dir = input.to_direction();
                                if dir != IVec2::ZERO {
                                    let next_pos = tile_pos.vec + dir;
                                    let walkable = if dir.x != 0 && dir.y != 0 {
                                        // diagonal: destination + both adjacent cardinals must be clear
                                        self.map.is_walkable(next_pos, &self.collision_grid)
                                            && self.map.is_walkable(
                                                tile_pos.vec + IVec2::new(dir.x, 0),
                                                &self.collision_grid,
                                            )
                                            && self.map.is_walkable(
                                                tile_pos.vec + IVec2::new(0, dir.y),
                                                &self.collision_grid,
                                            )
                                    } else {
                                        self.map.is_walkable(next_pos, &self.collision_grid)
                                    };

                                    if walkable {
                                        target_pos.vec = next_pos;
                                        target_pos.path = None;
                                        tile_pos.vec = next_pos;

                                        // diagonal costs 1.5x more than cardinal
                                        target_pos.delay =
                                            if dir.x != 0 && dir.y != 0 { 0.45 } else { 0.3 };

                                        let msg = ServerMessage::PlayerMoved {
                                            id,
                                            position: self.map.tiled.tile_to_world(tile_pos.vec),
                                            path: None,
                                        };
                                        self.server.broadcast(
                                            &serialize(&msg).unwrap(),
                                            Reliability::Unreliable,
                                        );
                                    }
                                }
                            }
                        }
                    }
                },
            }
        }
    }

    pub fn update(&mut self, dt: f32) {
        self.collision_grid.update(&self.map, &self.world);

        let moving: Vec<(NetId, Entity)> = self
            .lobby
            .iter()
            .map(|(&id, &(entity, _))| (id, entity))
            .collect();

        for (id, player) in moving {
            let mut target_pos = self.world.get_mut::<TargetTilePos>(player).unwrap();
            let mut tile_pos = self.world.get_mut::<TilePos>(player).unwrap();

            target_pos.delay -= dt;

            if tile_pos.vec == target_pos.vec && target_pos.delay <= 0.0 {
                if let Some(path) = target_pos.path.as_mut() {
                    if let Some(next) = path.first().copied() {
                        if self.map.is_walkable(next, &self.collision_grid) {
                            path.remove(0);
                            target_pos.vec = next;
                            tile_pos.vec = next;
                            target_pos.delay = 0.3; // matches client rate

                            let msg = ServerMessage::PlayerMoved {
                                id,
                                position: self.map.tiled.tile_to_world(tile_pos.vec),
                                path: None,
                            };
                            self.server
                                .broadcast(&serialize(&msg).unwrap(), Reliability::Unreliable);
                        } else {
                            target_pos.path = None;
                        }
                    }
                }
            }
        }
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
            TargetTilePos {
                vec: spawn_pos,
                path: None,
                delay: 0.0,
            },
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
