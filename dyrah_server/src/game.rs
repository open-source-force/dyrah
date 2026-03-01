use std::{collections::HashMap, path::Path, time::Duration};

use bincode::{deserialize, serialize};
use glam::IVec2;
use rand::Rng;
use secs::{Entity, World};
use wrym::{
    Reliability,
    server::{Server, ServerConfig, ServerEvent, Transport},
};

use dyrah_shared::{
    NetId,
    components::{Creature, Player},
    messages::{ClientMessage, CreatureMove, CreatureSpawn, ServerMessage},
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
        let server = Server::new(
            Transport::new("0.0.0.0:8080"),
            ServerConfig {
                client_timeout: Duration::from_mins(10),
            },
        );
        let map = Map::new("assets/map.json");
        let world = World::default();

        for (name, pos) in map.get_spawns() {
            if name == "player" {
                continue;
            }

            world.spawn((
                Creature { kind: name },
                TilePos { vec: pos },
                TargetTilePos {
                    vec: pos,
                    path: None,
                    delay: 0.0,
                },
            ));
        }

        Self {
            db: Database::new(path),
            server,
            pending: HashMap::new(),
            lobby: HashMap::new(),
            world,
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

        // player path following
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
                            target_pos.delay = 0.3;

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

        // collect player tile positions for creature AI
        let player_positions: Vec<IVec2> = self
            .lobby
            .values()
            .map(|&(entity, _)| self.world.get::<TilePos>(entity).unwrap().vec)
            .collect();

        let creatures: Vec<Entity> = {
            let mut v = Vec::new();
            self.world
                .query(|entity, _: &Creature, _: &TilePos, _: &TargetTilePos| {
                    v.push(entity);
                });
            v
        };
        let mut crea_moves = Vec::new();
        let mut rng = rand::thread_rng();

        for entity in creatures {
            let mut target_pos = self.world.get_mut::<TargetTilePos>(entity).unwrap();
            let mut tile_pos = self.world.get_mut::<TilePos>(entity).unwrap();

            target_pos.delay -= dt;

            if tile_pos.vec == target_pos.vec && target_pos.delay <= 0.0 {
                if let Some(path) = target_pos.path.as_mut() {
                    if let Some(next) = path.first().copied() {
                        if self.map.is_walkable(next, &self.collision_grid) {
                            let diagonal = next - tile_pos.vec;
                            path.remove(0);
                            target_pos.vec = next;
                            tile_pos.vec = next;
                            target_pos.delay = if diagonal.x != 0 && diagonal.y != 0 {
                                0.45
                            } else {
                                0.3
                            };
                            crea_moves.push(CreatureMove {
                                id: entity.id(),
                                position: self.map.tiled.tile_to_world(next),
                            });
                        } else {
                            target_pos.path = None;
                        }
                    } else {
                        target_pos.path = None; // path exhausted
                    }
                    continue;
                }

                let kind = self.world.get::<Creature>(entity).unwrap().kind.clone();
                let follow_range = match kind.as_str() {
                    "ghost" => 8,
                    "kitty" => 4,
                    _ => 0,
                };

                // find nearest player in 4 tiles
                let nearest = player_positions
                    .iter()
                    .filter_map(|&p| {
                        let diff = p - tile_pos.vec;
                        let dist = diff.x.abs().max(diff.y.abs());
                        if dist <= follow_range {
                            Some((dist, p))
                        } else {
                            None
                        }
                    })
                    .min_by_key(|(d, _)| *d)
                    .map(|(_, p)| p);

                if let Some(player_tile) = nearest {
                    let adjacent = [
                        IVec2::new(0, -1),
                        IVec2::new(0, 1),
                        IVec2::new(-1, 0),
                        IVec2::new(1, 0),
                    ]
                    .iter()
                    .map(|&d| player_tile + d)
                    .filter(|&t| self.map.is_walkable(t, &self.collision_grid))
                    .min_by_key(|&t| {
                        let diff = t - tile_pos.vec;
                        diff.x.abs() + diff.y.abs()
                    });

                    if let Some(target_tile) = adjacent {
                        if let Some(path) =
                            self.map
                                .find_path(tile_pos.vec, target_tile, &self.collision_grid)
                        {
                            target_pos.path = Some(path);
                        }
                    }
                } else {
                    let dir = IVec2::new(rng.gen_range(-1..=1), rng.gen_range(-1..=1));
                    if dir != IVec2::ZERO {
                        let next = tile_pos.vec + dir;
                        if self.map.is_walkable(next, &self.collision_grid) {
                            tile_pos.vec = next;
                            target_pos.vec = next;
                            target_pos.delay = if dir.x != 0 && dir.y != 0 { 0.45 } else { 0.3 };
                            crea_moves.push(CreatureMove {
                                id: entity.id(),
                                position: self.map.tiled.tile_to_world(next),
                            });
                        } else {
                            target_pos.delay = 0.5;
                        }
                    } else {
                        target_pos.delay = 1.0;
                    }
                }
            }
        }

        if !crea_moves.is_empty() {
            let msg = ServerMessage::CreatureBatchMoved(crea_moves);
            self.server
                .broadcast(&serialize(&msg).unwrap(), Reliability::Unreliable);
        }
    }

    fn spawn_player(&mut self, id: NetId, username: String, addr: &String) {
        let mut creatures = Vec::new();
        self.world
            .query(|_, creature: &Creature, tile_pos: &TilePos| {
                creatures.push(CreatureSpawn {
                    kind: creature.kind.clone(),
                    position: self.map.tiled.tile_to_world(tile_pos.vec),
                    health: 20.0,
                });
            });
        let msg = ServerMessage::CreatureBatchSpawned(creatures);
        self.server.send_to(
            addr,
            &serialize(&msg).unwrap(),
            Reliability::ReliableOrdered { channel: 0 },
        );

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
