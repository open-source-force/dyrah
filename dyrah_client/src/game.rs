use std::collections::HashMap;

use bincode::{deserialize, serialize};
use egor::{
    app::egui::*,
    input::{Input, KeyCode, MouseButton},
    math::{IVec2, Vec2},
    render::{Color, Graphics},
    time::FrameTimer,
};
use rand::{Rng, RngExt, SeedableRng, rngs::SmallRng};
use secs::{Entity, World};
use wrym::{
    Reliability,
    client::{Client, ClientEvent, Transport},
};

use dyrah_shared::{
    NetId, TILE_SIZE,
    components::{Creature, Health, Player},
    messages::{ClientInput, ClientMessage, ServerMessage},
};

use crate::{
    asset,
    components::{Sprite, TargetWorldPos, WorldPos},
    map::Map,
    sprite::Animation,
    systems::{movement, render},
    texture::TextureManager,
};

pub struct DamageNumber {
    pub position: Vec2,
    pub value: f32,
    pub age: f32,
}

pub struct SpellEffect {
    pub spell: String,
    pub origin: IVec2,
    pub affected_tiles: Vec<IVec2>,
    pub age: f32,
    pub duration: f32,
    pub seed: u64,
}

pub struct AuthState {
    pub username: String,
    pub password: String,
    pub error: Option<String>,
}

pub struct InputState {
    pub last_input_time: f32,
    pub dir_age: [f32; 4],
    pub move_start_grace: f32,
    pub was_moving: bool,
}

enum AppState {
    Auth,
    InGame,
}

pub struct Game {
    client: Client<Transport>,
    world: World,
    map: Map,
    lobby: HashMap<NetId, (Entity, String)>,
    textures: TextureManager,
    player: Option<Entity>,
    player_id: Option<NetId>,
    chat_messages: Vec<(String, String, f32)>,
    chat_input: String,
    app_state: AppState,
    auth_state: AuthState,
    input_state: InputState,
    hovered_tile: Option<IVec2>,
    spell_effects: Vec<SpellEffect>,
    damage_numbers: Vec<DamageNumber>,
}

impl Game {
    pub fn new() -> Self {
        Self {
            client: Client::new(Transport::new("127.0.0.1:8080"), "0.0.0.0:0"),
            world: World::default(),
            map: Map::new("assets/map.json"),
            lobby: HashMap::new(),
            textures: TextureManager::new(),
            player: None,
            player_id: None,
            chat_messages: Vec::new(),
            chat_input: String::new(),
            app_state: AppState::Auth,
            auth_state: AuthState {
                username: String::new(),
                password: String::new(),
                error: None,
            },
            input_state: InputState {
                last_input_time: 0.0,
                dir_age: [f32::MAX; 4],
                move_start_grace: 0.0,
                was_moving: false,
            },
            hovered_tile: None,
            spell_effects: Vec::new(),
            damage_numbers: Vec::new(),
        }
    }

    pub fn load(&mut self, gfx: &mut Graphics) {
        self.map.load(gfx);

        self.textures.load(gfx, "player", asset!("player.png"));
        self.textures.load(gfx, "kitty", asset!("kitty.png"));
        self.textures.load(gfx, "ghost", asset!("ghost.png"));
    }

    pub fn handle_events(&mut self) {
        self.client.poll();
        while let Some(event) = self.client.recv_event() {
            match event {
                ClientEvent::Connected(id) => {
                    println!("Connected to server!");
                    self.player_id = Some(id);
                }
                ClientEvent::Disconnected => {
                    println!("Lost connection to server");
                    self.app_state = AppState::Auth;
                    self.auth_state.error = Some("Lost connection to server".into());
                }
                ClientEvent::MessageReceived(bytes) => {
                    let msg = deserialize::<ServerMessage>(&bytes).unwrap();
                    self.handle_server_messages(msg);
                }
            }
        }
    }

    fn handle_server_messages(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::AuthSuccess { .. } => {
                // server handles this implicitly via PlayerSpawned for our own id,
                // but we transition state here if you add it explicitly
                self.app_state = AppState::InGame;
                self.auth_state.error = None;
            }
            ServerMessage::AuthFailed { reason } => {
                self.auth_state.error = Some(reason);
            }
            ServerMessage::PlayerSpawned {
                id,
                username,
                position,
                health,
                z,
            } => {
                println!("Player {} ({}) spawned!", id, username);

                let player = self.world.spawn((
                    Player,
                    WorldPos { vec: position, z },
                    TargetWorldPos {
                        vec: position,
                        path: None,
                    },
                    Sprite {
                        anim: Animation::new(3, 4, 0.2),
                    },
                    Health {
                        current: health,
                        max: health,
                    },
                ));

                self.lobby.insert(id, (player, username));
                if Some(id) == self.player_id {
                    self.player = Some(player);
                    self.map.current_z = z;
                    self.app_state = AppState::InGame;
                }
            }
            ServerMessage::PlayerDespawned { id } => {
                println!("Player {} disappeared", id);
                if let Some((player, _)) = self.lobby.remove(&id) {
                    self.world.despawn(player);
                }
            }
            ServerMessage::PlayerMoved {
                id,
                position,
                path,
                z,
            } => {
                if let Some((player, _)) = self.lobby.get(&id) {
                    let mut world_pos = self.world.get_mut::<WorldPos>(*player).unwrap();
                    let mut target_pos = self.world.get_mut::<TargetWorldPos>(*player).unwrap();
                    target_pos.vec = position;
                    world_pos.z = z;
                    if path.is_some() {
                        target_pos.path = path;
                    }
                    if Some(*player) == self.player {
                        self.map.current_z = z;
                    }
                }
            }
            ServerMessage::PlayerChangedFloor {
                id,
                position,
                floor,
            } => {
                if let Some((player, _)) = self.lobby.get(&id) {
                    let mut world_pos = self.world.get_mut::<WorldPos>(*player).unwrap();
                    let mut target_pos = self.world.get_mut::<TargetWorldPos>(*player).unwrap();
                    world_pos.vec = position;
                    world_pos.z = floor;
                    target_pos.vec = position;
                    target_pos.path = None;

                    if Some(*player) == self.player {
                        self.map.current_z = floor;
                    }
                }
            }
            ServerMessage::ChatReceived { sender_id, text } => {
                let username = self
                    .lobby
                    .get(&sender_id)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_else(|| sender_id.to_string());
                self.chat_messages.push((username, text, 0.0));
            }
            ServerMessage::CreatureBatchSpawned(spawns) => {
                for spawn in spawns {
                    let anim = match spawn.kind.as_str() {
                        "kitty" => Animation::new(4, 4, 0.15),
                        "ghost" => Animation::new(3, 4, 0.15),
                        kind => panic!("unknown creature kind: {}", kind),
                    };
                    self.world.spawn((
                        Creature { kind: spawn.kind },
                        WorldPos {
                            vec: spawn.position,
                            z: spawn.z,
                        },
                        TargetWorldPos {
                            vec: spawn.position,
                            path: None,
                        },
                        Sprite { anim },
                        Health {
                            current: spawn.health,
                            max: spawn.health,
                        },
                    ));
                }
            }
            ServerMessage::CreatureBatchMoved(moves) => {
                for m in moves {
                    if let Some(mut tgt_pos) = self.world.get_mut::<TargetWorldPos>(m.id.into()) {
                        tgt_pos.vec = m.position;
                    }
                }
            }
            ServerMessage::EntitiesDamaged { entries } => {
                for entry in entries {
                    if let Some(mut health) = self.world.get_mut::<Health>(entry.id.into()) {
                        health.current = entry.current;
                        health.max = entry.max;
                    }

                    if let Some(pos) = self.world.get::<WorldPos>(entry.id.into()) {
                        self.damage_numbers.push(DamageNumber {
                            position: pos.vec,
                            value: entry.damage,
                            age: 0.0,
                        });
                    }
                }
            }
            ServerMessage::EntitiesDied { ids } => {
                for id in ids {
                    self.world.despawn(id.into());
                }
            }
            ServerMessage::SpellCast {
                caster_id,
                spell,
                origin,
                affected_tiles,
                hit_entities,
            } => {
                self.spell_effects.push(SpellEffect {
                    spell,
                    origin,
                    affected_tiles,
                    age: 0.0,
                    duration: 1.5,
                    seed: rand::random::<u64>(),
                });
            }
        }
    }

    pub fn update(
        &mut self,
        gfx: &mut Graphics,
        input: &Input,
        egui_ctx: &mut &Context,
        timer: &FrameTimer,
    ) {
        self.world.flush_despawned();

        if let Some(player) = self.player {
            if let Some(pos) = self.world.get::<WorldPos>(player) {
                let screen = gfx.screen_size();
                gfx.camera().set_zoom(2.0);
                gfx.camera().center(pos.vec, screen);
            }
        }

        if matches!(self.app_state, AppState::Auth) {
            return;
        }

        let dt = timer.delta;

        self.damage_numbers.retain_mut(|d| {
            d.age += dt;
            d.age < 1.0
        });
        self.spell_effects.retain_mut(|e| {
            e.age += dt;
            e.age < e.duration
        });

        movement::update(&mut self.world, dt);

        self.chat_messages.retain_mut(|(_, _, age)| {
            *age += dt;
            *age < 10.0
        });

        let egui_focused = egui_ctx.wants_keyboard_input();
        let raw_dirs = if egui_focused {
            [false; 4]
        } else {
            [
                input.keys_held(&[KeyCode::KeyA, KeyCode::ArrowLeft]),
                input.keys_held(&[KeyCode::KeyW, KeyCode::ArrowUp]),
                input.keys_held(&[KeyCode::KeyD, KeyCode::ArrowRight]),
                input.keys_held(&[KeyCode::KeyS, KeyCode::ArrowDown]),
            ]
        };

        let diagonal_buffer = 3.0 / 60.0;
        for i in 0..4 {
            if raw_dirs[i] {
                self.input_state.dir_age[i] = 0.0;
            } else {
                self.input_state.dir_age[i] += dt;
            }
        }

        let buffered = [
            self.input_state.dir_age[0] < diagonal_buffer,
            self.input_state.dir_age[1] < diagonal_buffer,
            self.input_state.dir_age[2] < diagonal_buffer,
            self.input_state.dir_age[3] < diagonal_buffer,
        ];
        let [left, up, right, down] = buffered;

        let mouse_world = gfx.camera().screen_to_world(input.mouse_position().into());
        let mouse_tile = self.map.tiled.world_to_tile(mouse_world);

        self.hovered_tile = Some(mouse_tile);

        let left_click = input
            .mouse_released(MouseButton::Left)
            .then_some(mouse_tile);
        let right_click = input
            .mouse_released(MouseButton::Right)
            .then_some(mouse_tile);

        if let Some(tile) = left_click {
            self.input_state.last_input_time = 0.0;
            self.client.send(
                &serialize(&ClientMessage::PlayerUpdate {
                    input: ClientInput {
                        left: false,
                        up: false,
                        right: false,
                        down: false,
                        left_click: Some(tile),
                        right_click: None,
                    },
                })
                .unwrap(),
                Reliability::Unreliable,
            );

            return;
        }

        if let Some(tile) = right_click {
            self.input_state.last_input_time = 0.0;
            self.client.send(
                &serialize(&ClientMessage::PlayerUpdate {
                    input: ClientInput {
                        left: false,
                        up: false,
                        right: false,
                        down: false,
                        left_click: None,
                        right_click: Some(tile),
                    },
                })
                .unwrap(),
                Reliability::Unreliable,
            );

            return;
        }

        self.input_state.last_input_time += dt;

        let keyboard_moving = left || up || right || down;

        if keyboard_moving && !self.input_state.was_moving {
            self.input_state.move_start_grace = diagonal_buffer;
        }

        self.input_state.was_moving = keyboard_moving;

        if self.input_state.move_start_grace > 0.0 {
            self.input_state.move_start_grace -= dt;
        }

        if self.input_state.last_input_time >= 0.3
            && keyboard_moving
            && self.input_state.move_start_grace <= 0.0
        {
            self.input_state.last_input_time = 0.0;
            self.client.send(
                &serialize(&ClientMessage::PlayerUpdate {
                    input: ClientInput {
                        left,
                        up,
                        right,
                        down,
                        left_click: None,
                        right_click: None,
                    },
                })
                .unwrap(),
                Reliability::Unreliable,
            );
        }

        if !egui_focused && input.key_pressed(KeyCode::KeyF) {
            self.client.send(
                &serialize(&ClientMessage::CastSpell {
                    spell: "exevo gran mas flam".into(),
                })
                .unwrap(),
                Reliability::ReliableOrdered { channel: 0 },
            );
        }
    }

    pub fn render(&mut self, gfx: &mut Graphics, egui_ctx: &mut &Context, timer: &FrameTimer) {
        match self.app_state {
            AppState::Auth => self.render_auth(egui_ctx),
            AppState::InGame => self.render_game(gfx, egui_ctx, timer),
        }
    }

    fn render_auth(&mut self, egui_ctx: &mut &Context) {
        CentralPanel::default().show(egui_ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(100.0);
                ui.heading("Dyrah");
                ui.add_space(20.0);

                ui.label("Username");
                ui.text_edit_singleline(&mut self.auth_state.username);
                ui.add_space(5.0);

                ui.label("Password");
                ui.add(TextEdit::singleline(&mut self.auth_state.password).password(true));
                ui.add_space(10.0);

                if let Some(err) = &self.auth_state.error {
                    ui.colored_label(Color32::RED, err);
                    ui.add_space(5.0);
                }

                ui.horizontal(|ui| {
                    if ui.button("Login").clicked() {
                        let msg = ClientMessage::Login {
                            username: self.auth_state.username.clone(),
                            password: self.auth_state.password.clone(),
                        };
                        self.client.send(
                            &serialize(&msg).unwrap(),
                            Reliability::ReliableOrdered { channel: 0 },
                        );
                    }
                    if ui.button("Register").clicked() {
                        let msg = ClientMessage::Register {
                            username: self.auth_state.username.clone(),
                            password: self.auth_state.password.clone(),
                        };
                        self.client.send(
                            &serialize(&msg).unwrap(),
                            Reliability::ReliableOrdered { channel: 0 },
                        );
                    }
                });
            });
        });
    }

    fn render_game(&mut self, gfx: &mut Graphics, egui_ctx: &mut &Context, timer: &FrameTimer) {
        let screen = gfx.screen_size();
        gfx.clear(Color::BLUE);

        let player_tile = self
            .player
            .and_then(|p| self.world.get::<WorldPos>(p))
            .map(|pos| self.map.tiled.world_to_tile(pos.vec))
            .unwrap_or(IVec2::ZERO);
        self.map.draw_tiles(gfx, player_tile);

        render::creatures(&self.world, gfx, &self.textures);
        let player_world_pos = render::players(&self.world, gfx, &self.textures, self.player);

        self.world.query(|_, health: &Health, pos: &WorldPos| {
            let screen_pos = pos.vec;
            let bar_width = 24.0;
            let bar_height = 4.0;
            let hp_frac = (health.current / health.max).clamp(0.0, 1.0);
            let bar_x = screen_pos.x - bar_width / 2.0;
            let bar_y = screen_pos.y - 20.0; // above sprite

            // background
            gfx.rect()
                .at(Vec2::new(bar_x, bar_y))
                .size(Vec2::new(bar_width, bar_height))
                .color(Color::new([0.1, 0.1, 0.1, 1.0]));

            // foreground
            gfx.rect()
                .at(Vec2::new(bar_x, bar_y))
                .size(Vec2::new(bar_width * hp_frac, bar_height))
                .color(Color::new([0.8, 0.2, 0.2, 1.0]));
        });

        if let Some(path) = self
            .player
            .and_then(|p| self.world.get::<TargetWorldPos>(p))
            .and_then(|t| t.path.clone())
        {
            render::path(gfx, &path);
        }

        if let Some(tile) = self.hovered_tile {
            render::hover_tile(gfx, tile, &self.map);
        }

        spell_effects(gfx, &self.spell_effects, &self.map);
        for d in &self.damage_numbers {
            let offset = Vec2::new(0.0, -d.age * 20.0);
            let alpha = 1.0 - d.age;

            let screen_pos = gfx.camera().world_to_screen(d.position + offset);
            gfx.text(&format!("{}", d.value as i32))
                .at(screen_pos)
                .size(20.0)
                .color(Color::new([1.0, 0.3, 0.3, alpha]));
        }

        let latest_msgs = self.build_latest_msgs();
        render::chat_bubbles(&self.world, gfx, &latest_msgs, player_world_pos, screen);
        self.render_chat(egui_ctx, screen);

        render::debug(
            gfx,
            &self.world,
            self.player,
            &self.map,
            self.hovered_tile,
            timer.fps,
        );
    }

    fn build_latest_msgs(&self) -> HashMap<Entity, (String, f32)> {
        let mut latest = HashMap::new();
        for (username, text, age) in &self.chat_messages {
            if let Some((entity, _)) = self.lobby.values().find(|(_, name)| name == username) {
                latest.insert(*entity, (text.clone(), *age));
            }
        }
        latest
    }

    fn render_chat(&mut self, egui_ctx: &mut &Context, screen: Vec2) {
        let chat_width = screen.x / 2.0;
        let chat_height = screen.y / 5.0;
        let chat_x = 10.0;
        let chat_y = screen.y - chat_height - 10.0;

        Window::new("Chat")
            .resizable(false)
            .collapsible(false)
            .fixed_pos([chat_x, chat_y])
            .fixed_size([chat_width, chat_height])
            .show(egui_ctx, |ui| {
                let scroll_height = chat_height - 50.0;
                ScrollArea::vertical()
                    .max_height(scroll_height)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (username, text, _) in &self.chat_messages {
                            ui.label(format!("{}: {}", username, text));
                        }
                    });

                ui.separator();
                let response = ui.text_edit_singleline(&mut self.chat_input);

                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let text = self.chat_input.trim().to_string();
                    if !text.is_empty() {
                        let msg = ClientMessage::ChatMessage { text };
                        self.client
                            .send(&serialize(&msg).unwrap(), Reliability::Unreliable);
                        self.chat_input.clear();
                    }
                    egui_ctx.memory_mut(|memory| memory.surrender_focus(response.id));
                } else if !response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    response.request_focus();
                }
            });
    }
}

pub fn spell_effects(gfx: &mut Graphics, effects: &[SpellEffect], map: &crate::map::Map) {
    for effect in effects {
        let progress = effect.age / effect.duration; // 0.0 -> 1.0
        let mut rng = SmallRng::seed_from_u64(effect.seed);

        for &tile in &effect.affected_tiles {
            let world_pos = map.tiled.tile_to_world(tile);
            let center = world_pos + Vec2::splat(TILE_SIZE / 2.0);
            let dist = (tile - effect.origin).as_vec2().length();
            let max_dist = effect
                .affected_tiles
                .iter()
                .map(|&t| (t - effect.origin).as_vec2().length())
                .fold(0.0_f32, f32::max);

            // per-tile unique phase from seed + tile position
            let tile_seed: u64 = rng.next_u64();
            let phase = (tile_seed as f32 / u64::MAX as f32) * std::f32::consts::TAU;

            // fade out over time, stronger at center
            let dist_factor = 1.0 - (dist / (max_dist + 1.0));
            let flicker = (effect.age * 12.0 + phase).sin() * 0.3 + 0.7;
            let alpha = (1.0 - progress).powf(0.5) * dist_factor * flicker;

            // color: white core -> yellow -> orange -> red at edges
            let color = lerp_fire_color(dist_factor, alpha);

            // core flame polygon
            let core_radius = TILE_SIZE * 0.3 * dist_factor * flicker;
            let rotation = effect.age * 3.0 + phase;
            gfx.polygon()
                .at(center)
                .radius(core_radius)
                .segments(6)
                .rotate(rotation)
                .color(color);

            // inner bright core
            gfx.polygon()
                .at(center)
                .radius(core_radius * 0.4)
                .segments(6)
                .rotate(-rotation * 1.5)
                .color(Color::new([1.0, 1.0, 0.8, alpha]));

            // ember particles — 3 per tile, orbit outward then fade
            for i in 0..3 {
                let ember_seed = rng.random_range(0.0_f32..1.0);
                let ember_phase = phase + i as f32 * std::f32::consts::TAU / 3.0;
                let orbit = TILE_SIZE * 0.4 * progress * ember_seed;
                let ember_pos =
                    center + Vec2::new(ember_phase.cos() * orbit, ember_phase.sin() * orbit);
                let ember_alpha = alpha * (1.0 - progress) * ember_seed;
                gfx.polygon()
                    .at(ember_pos)
                    .radius(TILE_SIZE * 0.06 * flicker)
                    .segments(4)
                    .color(Color::new([1.0, 0.5, 0.1, ember_alpha]));
            }

            // flame tongue polyline using PathBuilder-style points
            let tongue_points: Vec<Vec2> = (0..5)
                .map(|i| {
                    let t = i as f32 / 4.0;
                    let wobble =
                        (effect.age * 8.0 + phase + t * 3.0).sin() * TILE_SIZE * 0.15 * (1.0 - t);
                    center + Vec2::new(wobble, -TILE_SIZE * 0.4 * t * dist_factor)
                })
                .collect();

            gfx.polyline()
                .points(&tongue_points)
                .thickness(TILE_SIZE * 0.12 * flicker * dist_factor)
                .color(Color::new([1.0, 0.7, 0.0, alpha * 0.8]));
        }
    }
}

fn lerp_fire_color(dist_factor: f32, alpha: f32) -> Color {
    // center: near white-yellow, edges: deep red
    let r = 1.0;
    let g = (0.8 * dist_factor).max(0.1);
    let b = (0.4 * dist_factor).max(0.0) * dist_factor;
    Color::new([r, g, b, alpha])
}
