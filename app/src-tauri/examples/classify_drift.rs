//! Dev diagnostic: re-run the rule engine against the CURRENT caches and report
//! how far the saved classifications have drifted from what the rules would say
//! today. Saved classifications are sticky (only new games get classified), so
//! store-detail backfills never reach games already on file.
//!
//!     cargo run --example classify_drift

use gamekeeper_lib::{cache, classifier, config};
use std::collections::HashMap;

fn main() {
    let cfg = config::load_config().expect("config (run the app once first)");
    let games = cache::load_library_cache_any_age(&cfg.steam_id).expect("library cache");
    let store_cache = cache::load_store_cache();
    let saved = cache::load_saved_classifications();

    println!(
        "{} games, {} store entries, {} saved classifications",
        games.len(),
        store_cache.len(),
        saved.len()
    );

    let mut drift: HashMap<(String, String), Vec<(String, String)>> = HashMap::new();
    let mut unchanged = 0;
    let mut fresh_counts: HashMap<String, usize> = HashMap::new();
    let mut saved_counts: HashMap<String, usize> = HashMap::new();

    for game in &games {
        let store = store_cache.get(&game.appid.to_string());
        let (category, reason) = classifier::classify_by_rules(game, store);
        let fresh = category.to_string();
        *fresh_counts.entry(fresh.clone()).or_default() += 1;

        let Some(old) = saved.get(&game.appid) else {
            continue;
        };
        let was = old.category.to_string();
        *saved_counts.entry(was.clone()).or_default() += 1;

        if was == fresh {
            unchanged += 1;
        } else {
            drift
                .entry((was, fresh))
                .or_default()
                .push((game.name.clone(), reason));
        }
    }

    println!("\nsaved on disk : {saved_counts:?}");
    println!("rules today   : {fresh_counts:?}");
    println!("\nunchanged: {unchanged}");

    let mut keys: Vec<_> = drift.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let items = &drift[&key];
        println!("\n{} -> {} : {} games", key.0, key.1, items.len());
        for (name, reason) in items.iter().take(6) {
            println!("   {name}  [{reason}]");
        }
        if items.len() > 6 {
            println!("   ... and {} more", items.len() - 6);
        }
    }
}
