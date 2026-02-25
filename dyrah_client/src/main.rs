mod components;
mod game;
mod map;
mod sprite;

use egor::app::{App, FrameContext};

use crate::game::Game;

fn main() {
    let mut game = Game::new();

    App::new().title("Dyrah").vsync(false).run(
        move |FrameContext {
                  gfx,
                  input,
                  timer,
                  egui_ctx,
                  ..
              }| {
            if timer.frame == 0 {
                game.load(gfx);
            }

            game.handle_events();
            game.update(gfx, input, egui_ctx, timer);
            game.render(gfx, egui_ctx, timer);
        },
    );
}
