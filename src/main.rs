use ggez::event::{self, EventHandler};
use ggez::glam::Vec2;
use ggez::graphics::{self, Color};
use ggez::input::gamepad::Gilrs;
use ggez::input::gamepad::gilrs::GilrsBuilder;
use ggez::winit::event::MouseButton;
use ggez::winit::keyboard::{KeyCode, PhysicalKey};
use ggez::{Context, ContextBuilder, GameResult};

struct MyGame {
    gilrs: Gilrs,
    pos_x: f32,
}

impl MyGame {
    pub fn new(_ctx: &mut Context) -> MyGame {
        // Load/create resources such as images here.
        let gilrs = GilrsBuilder::new().with_default_filters(false).build().unwrap();
        MyGame { gilrs, pos_x: 0.0 }
    }
}

impl EventHandler for MyGame {
    fn update(&mut self, ctx: &mut Context) -> GameResult {
        // Update code here...
        while let Some(event) = self.gilrs.next_event() {
            println!("{:?}", event);
        }

        if ctx.mouse.button_just_pressed(MouseButton::Left) {
            println!("Left Click");
        }

        if ctx.mouse.button_just_pressed(MouseButton::Right) {
            println!("Right Click");
        }

        if ctx
            .keyboard
            .is_physical_key_just_pressed(&PhysicalKey::Code(KeyCode::Enter))
        {
            println!("Enter");
        }

        if ctx
            .keyboard
            .is_physical_key_just_pressed(&PhysicalKey::Code(KeyCode::Space))
        {
            println!("Space");
        }

        self.pos_x += 1.0;

        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        let mut canvas = graphics::Canvas::from_frame(ctx, graphics::Color::from([0.1, 0.2, 0.3, 1.0]));

        let circle = graphics::Mesh::new_circle(
            ctx,
            graphics::DrawMode::fill(),
            Vec2::new(0.0, 0.0),
            100.0,
            2.0,
            Color::WHITE,
        )?;
        canvas.draw(&circle, Vec2::new(self.pos_x, 380.0));

        canvas.finish(ctx)
    }
}

fn main() -> Result<(), ggez::GameError> {
    // Make a Context.
    let (mut ctx, event_loop) = ContextBuilder::new("my_game", "Cool Game Author")
        .build()
        .expect("aieee, could not create ggez context!");

    // Create an instance of your event handler.
    // Usually, you should provide it with the Context object to
    // use when setting your game up.
    let my_game = MyGame::new(&mut ctx);

    // Run!
    event::run(ctx, event_loop, my_game)
}
