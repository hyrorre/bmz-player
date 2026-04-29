use bmz_core::input::InputEvent;

use super::backend::InputBackend;
use super::translator::{InputTimingContext, InputTranslator};

pub struct InputSystem {
    pub backend: Box<dyn InputBackend>,
    pub translator: Box<dyn InputTranslator>,
}

impl InputSystem {
    pub fn collect_game_inputs(&mut self, ctx: &InputTimingContext<'_>) -> Vec<InputEvent> {
        self.backend.update();
        self.backend
            .drain_events()
            .into_iter()
            .filter_map(|event| self.translator.translate(event, ctx))
            .collect()
    }
}
