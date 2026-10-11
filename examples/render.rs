//! Renders a skin with a fake playing state to a PNG, for eyeballing
//! against Webamp: `cargo run --example render -- skin.wsz out.png [base.wsz]`.

use std::path::Path;

use llama::cliamp::Track;
use llama::cliamp::engine::Live;
use llama::skin::Skin;
use llama::ui::{Ui, View};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let skin = Skin::load(Path::new(&args[1]))?;
    let skin = match args.get(3) {
        Some(base) => skin.with_fallback(&Skin::load(Path::new(base))?),
        None => skin,
    };
    let mut live = Live {
        connected: true,
        ..Default::default()
    };
    live.snap.state = "playing".into();
    live.snap.position = 83.0;
    live.snap.duration = 225.0;
    live.snap.volume = Some(-6.0);
    live.snap.repeat = "all".into();
    live.snap.eq_bands = vec![5.0, 4.0, 2.0, -1.0, -2.0, 2.0, 4.0, 5.0, 5.0, 5.0];
    live.snap.index = 1;
    live.snap.total = 4;
    live.spectrum = vec![0.9, 0.8, 0.7, 0.75, 0.5, 0.4, 0.45, 0.3, 0.2, 0.1];
    live.queue = [
        ("Nullsoft", "It Really Whips the Llama's Ass", 5.0),
        ("Daft Punk", "Harder, Better, Faster, Stronger", 225.0),
        ("Radiohead", "Paranoid Android", 383.0),
        ("Aphex Twin", "Windowlicker", 367.0),
    ]
    .iter()
    .map(|(a, t, d)| Track {
        artist: a.to_string(),
        title: t.to_string(),
        duration: Some(*d),
        ..Default::default()
    })
    .collect();
    live.snap.track = Some(live.queue[1].clone());
    let mut ui = Ui::new(skin, View::default());
    ui.view.pl_sel.insert(2);
    ui.draw(&live);
    ui.draw(&live); // a second frame lets the peaks settle
    ui.to_png(Path::new(&args[2]), 3)?;
    Ok(())
}
