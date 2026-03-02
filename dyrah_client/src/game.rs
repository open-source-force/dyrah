use std::collections::HashMap;

use bincode::{deserialize, serialize};
use egor::{
    app::egui::*,
    input::{Input, KeyCode, MouseButton},
    math::{IVec2, Vec2},
    render::{Color, Graphics},
    time::FrameTimer,
};
use secs::{Entity, World};
use wrym::{
    Reliability,
    client::{Client, ClientEvent, Transport},
};

use dyrah_shared::{
    NetId,
    components::{Creature, Player},
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
            } => {
                println!("Player {} ({}) spawned!", id, username);

                let player = self.world.spawn((
                    Player,
                    WorldPos { vec: position },
                    TargetWorldPos {
                        vec: position,
                        path: None,
                    },
                    Sprite {
                        anim: Animation::new(3, 4, 0.2),
                    },
                ));

                self.lobby.insert(id, (player, username));
                if Some(id) == self.player_id {
                    self.player = Some(player);
                    self.app_state = AppState::InGame;
                }
            }
            ServerMessage::PlayerDespawned { id } => {
                println!("Player {} disappeared", id);
                if let Some((player, _)) = self.lobby.remove(&id) {
                    self.world.despawn(player);
                }
            }
            ServerMessage::PlayerMoved { id, position, path } => {
                if let Some((player, _)) = self.lobby.get(&id) {
                    let mut target_pos = self.world.get_mut::<TargetWorldPos>(*player).unwrap();
                    target_pos.vec = position;
                    if path.is_some() {
                        target_pos.path = path;
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
                        },
                        TargetWorldPos {
                            vec: spawn.position,
                            path: None,
                        },
                        Sprite { anim },
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

        let mouse_world_pos = gfx.camera().screen_to_world(input.mouse_position().into());
        let mouse_tile_pos = input
            .mouse_released(MouseButton::Left)
            .then_some(mouse_world_pos)
            .map(|mp| self.map.tiled.world_to_tile(mp.into()));
        self.hovered_tile = Some(self.map.tiled.world_to_tile(mouse_world_pos));

        self.input_state.last_input_time += dt;

        if mouse_tile_pos.is_some() {
            self.input_state.last_input_time = 0.0;
            self.client.send(
                &serialize(&ClientMessage::PlayerUpdate {
                    input: ClientInput {
                        left: false,
                        up: false,
                        right: false,
                        down: false,
                        mouse_tile_pos,
                    },
                })
                .unwrap(),
                Reliability::Unreliable,
            );
        } else {
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
                            mouse_tile_pos: None,
                        },
                    })
                    .unwrap(),
                    Reliability::Unreliable,
                );
            }
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
