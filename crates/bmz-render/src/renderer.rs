use anyhow::Result;

use crate::scene::AppSceneSnapshot;

#[derive(Debug, Default)]
pub struct Renderer {
    last_scene: Option<AppSceneSnapshot>,
}

impl Renderer {
    pub fn render_scene(&mut self, scene: AppSceneSnapshot) -> Result<()> {
        self.last_scene = Some(scene);
        Ok(())
    }

    pub fn last_scene(&self) -> Option<&AppSceneSnapshot> {
        self.last_scene.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use crate::scene::{AppSceneSnapshot, SelectSnapshot};

    use super::*;

    #[test]
    fn renderer_records_last_scene() {
        let mut renderer = Renderer::default();
        let scene = AppSceneSnapshot::Select(SelectSnapshot {
            chart_count: 1,
            selected_index: 0,
            selected_chart_id: Some(7),
            selected_title: "test".to_string(),
        });

        renderer.render_scene(scene.clone()).unwrap();

        assert_eq!(renderer.last_scene(), Some(&scene));
    }
}
