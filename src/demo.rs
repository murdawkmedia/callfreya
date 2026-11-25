use crate::Speech;
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Serialize)]
pub struct Demo {
    product: String,
    intro: String,
    steps: Vec<Step>,
}

#[derive(Clone, Deserialize, Serialize)]
struct Step {
    title: String,
    body: String,
}

impl Demo {
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let demo: Self = match std::env::var_os("CALLFREYA_DEMO_FILE") {
            Some(path) => serde_json::from_reader(std::fs::File::open(path)?)?,
            None => serde_json::from_str(include_str!("demo.json"))?,
        };
        let valid_text = |text: &str, limit| !text.trim().is_empty() && text.len() <= limit;
        if !valid_text(&demo.product, 120)
            || !valid_text(&demo.intro, 500)
            || !(1..=12).contains(&demo.steps.len())
            || demo
                .steps
                .iter()
                .any(|step| !valid_text(&step.title, 120) || !valid_text(&step.body, 1500))
        {
            return Err(
                "Invalid demo: provide product, intro and one to twelve nonblank steps".into(),
            );
        }
        Ok(demo)
    }
}

pub async fn content(State(speech): State<Speech>) -> Json<Demo> {
    Json(speech.demo)
}
