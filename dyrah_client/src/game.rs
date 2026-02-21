use std::collections::HashMap;

use bincode::{deserialize, serialize};
use egor::{
    app::egui::*,
    input::{Input, KeyCode, MouseButton},
    math::Vec2,
    render::{Color, Graphics},
    time::FrameTimer,
};
use secs::{Entity, World};
use wrym::{
    client::{Client, ClientEvent},
    transport::Transport,
};

use dyrah_shared::{
    NetId,
    components::Player,
    messages::{ClientInput, ClientMessage, ServerMessage},
};

use crate::{
    components::{Sprite, TargetWorldPos, WorldPos},
    map::Map,
    sprite::Animation,
};

pub struct Game {
    client: Client<Transport>,
    world: World,
    map: Map,
    lobby: HashMap<NetId, Entity>,
    last_input_time: f32,
    player_tex: Option<usize>,
    player: Option<Entity>,
    player_id: Option<NetId>,
    chat_messages: Vec<(NetId, String)>,
    chat_input: String,
    chat_open: bool,
}

impl Game {
    pub fn new() -> Self {
        Self {
            client: Client::new(Transport::new("127.0.0.1:0"), "127.0.0.1:8080"),
            world: World::default(),
            map: Map::new("assets/map.json"),
            lobby: HashMap::new(),
            last_input_time: 0.0,
            player_tex: None,
            player: None,
            player_id: None,
            chat_messages: Vec::new(),
            chat_input: String::new(),
            chat_open: false,
        }
    }

    pub fn load(&mut self, gfx: &mut Graphics) {
        self.map.load(gfx);
        self.player_tex = Some(gfx.load_texture(include_bytes!("../../assets/wizard.png")));
    }

    pub fn handle_events(&mut self) {
        while let Some(event) = self.client.recv_event() {
            match event {
                ClientEvent::Connected(id) => {
                    println!("Connected to server!");
                    self.player_id = Some(id);
                }
                ClientEvent::Disconnected => {
                    println!("Lost connection to server");
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
            ServerMessage::PlayerSpawned { id, position } => {
                println!("Player {} spawned!", id);

                let player = self.world.spawn((
                    Player,
                    WorldPos { vec: position },
                    TargetWorldPos { vec: position },
                    Sprite {
                        anim: Animation::new(1, 6, 6, 0.2),
                        frame_size: Vec2::splat(64.0),
                        sprite_size: Vec2::new(32.0, 64.0),
                    },
                ));

                self.lobby.insert(id, player);
                if Some(id) == self.player_id {
                    self.player = Some(player);
                }
            }
            ServerMessage::PlayerDespawned { id } => {
                println!("Player {} disappeared", id);
                self.lobby.remove(&id).map(|p| self.world.despawn(p));
            }
            ServerMessage::PlayerMoved { id, position } => {
                if let Some(&player) = self.lobby.get(&id) {
                    let mut target_pos = self.world.get_mut::<TargetWorldPos>(player).unwrap();
                    target_pos.vec = position;
                }
            }
            ServerMessage::ChatReceived { sender_id, text } => {
                self.chat_messages.push((sender_id, text));
            }
        }
    }

    pub fn update(&mut self, input: &Input, egui_ctx: &mut &Context, timer: &FrameTimer) {
        self.client.poll();

        let egui_focused = egui_ctx.wants_keyboard_input();
        let left = if egui_focused { false } else { input.keys_held(&[KeyCode::KeyA, KeyCode::ArrowLeft]) };
        let up = if egui_focused { false } else { input.keys_held(&[KeyCode::KeyW, KeyCode::ArrowUp]) };
        let right = if egui_focused { false } else { input.keys_held(&[KeyCode::KeyD, KeyCode::ArrowRight]) };
        let down = if egui_focused { false } else { input.keys_held(&[KeyCode::KeyS, KeyCode::ArrowDown]) };

        let mouse_pos = input.mouse_position();
        let mouse_tile_pos = input
            .mouse_released(MouseButton::Left)
            .then_some(mouse_pos)
            .map(|mp| self.map.tiled.world_to_tile(mp.into()));
        let moving = left || up || right || down || mouse_tile_pos.is_some();

        self.world.query(
            |_, _: &Player, pos: &mut WorldPos, target_pos: &TargetWorldPos, spr: &mut Sprite| {
                if pos.vec != target_pos.vec {
                    pos.vec = pos.vec.lerp(target_pos.vec, 0.1);

                    let delta = target_pos.vec - pos.vec;
                    if delta.x.abs() > delta.y.abs() {
                        spr.anim.flip_x(delta.x < 0.0);
                    }

                    spr.anim.update(timer.delta);

                    if pos.vec.distance(target_pos.vec) < 1.0 {
                        pos.vec = target_pos.vec;
                    }
                } else {
                    spr.anim.set_frame(0);
                }
            },
        );

        self.last_input_time += timer.delta;
        if self.last_input_time >= 0.2 && moving {
            self.last_input_time = 0.0;

            let msg = ClientMessage::PlayerUpdate {
                input: ClientInput {
                    left,
                    up,
                    right,
                    down,
                    mouse_tile_pos,
                },
            };
            self.client.send(&serialize(&msg).unwrap());
        }
    }

    pub fn render(&mut self, gfx: &mut Graphics, egui_ctx: &mut &Context, timer: &FrameTimer) {
        let screen = gfx.screen_size();
        gfx.clear(Color::BLUE);

        self.map.draw_tiles(gfx);

        let mut latest_msgs: HashMap<Entity, &String> = HashMap::new();
        for (sender_id, text) in self.chat_messages.iter().rev() {
            if let Some(&entity) = self.lobby.get(sender_id) {
                latest_msgs.entry(entity).or_insert(text);
            }
        }

        let mut player_world_pos = Vec2::ZERO;

        self.world.query(|player, _: &Player, world_pos: &WorldPos, spr: &Sprite| {
            let draw_pos = world_pos.vec + spr.anim.offset(spr.frame_size, spr.sprite_size);
            gfx.rect()
                .at(draw_pos)
                .texture(self.player_tex.unwrap())
                .uv(spr.anim.frame());

            if Some(player) == self.player {
                player_world_pos = world_pos.vec;
                gfx.camera().center(world_pos.vec, screen);
            }
        });

        self.world.query(|player, _: &Player, world_pos: &WorldPos, _: &Sprite| {
            if let Some(msg) = latest_msgs.get(&player) {
                let screen_pos = world_pos.vec - player_world_pos + screen / 2.0;
                gfx.text(msg)
                    .at((screen_pos.x, screen_pos.y - 10.0))
                    .color(Color::WHITE);
            }
        });

        gfx.text(&format!("FPS: {}", timer.fps)).at((10.0, 10.0));

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
                ScrollArea::vertical()
                    .max_height(chat_height - 50.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for (sender, text) in self.chat_messages.iter() {
                            ui.label(format!("{}: {}", sender, text));
                        }
                    });

                ui.separator();

                let response = ui.text_edit_singleline(&mut self.chat_input);
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    let text = self.chat_input.trim().to_string();
                    if !text.is_empty() {
                        let msg = ClientMessage::ChatMessage { text };
                        self.client.send_reliable(&serialize(&msg).unwrap(), None);
                        self.chat_input.clear();
                    }
                    response.request_focus();
                }
            });
    }
}
