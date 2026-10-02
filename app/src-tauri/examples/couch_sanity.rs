//! Dev sanity check: run the couch / TV profiler against the REAL local caches
//! and print the breakdown plus a sample of what the Steam collection would
//! hold. Not a test — output needs eyeballs.
//!
//!     cargo run --example couch_sanity

use gamekeeper_lib::{cache, couch, couch::ControllerSupport};

fn main() {
    let classifications: Vec<_> = cache::load_saved_classifications().into_values().collect();
    let store_cache = cache::load_store_cache();
    println!(
        "{} classifications, {} store entries",
        classifications.len(),
        store_cache.len()
    );

    let appids: Vec<u64> = classifications.iter().map(|c| c.appid).collect();
    let profiles = couch::build_profiles(&appids, &store_cache);

    let mut full = 0;
    let mut partial = 0;
    let mut none = 0;
    let mut unknown = 0;
    let mut split = 0;
    let mut remote_tv = 0;
    let mut vr_only = 0;
    for p in profiles.values() {
        match p.controller {
            ControllerSupport::Full => full += 1,
            ControllerSupport::Partial => partial += 1,
            ControllerSupport::None => none += 1,
            ControllerSupport::Unknown => unknown += 1,
        }
        if p.split_screen {
            split += 1;
        }
        if p.remote_play_tv {
            remote_tv += 1;
        }
        if p.vr_only {
            vr_only += 1;
        }
    }
    println!("full {full}  partial {partial}  none {none}  unknown {unknown}");
    println!("split screen {split}  remote play on TV {remote_tv}  VR only {vr_only}");

    for include_partial in [false, true] {
        let mut picks: Vec<&str> = classifications
            .iter()
            .filter(|c| c.category.to_string() != "NOT_A_GAME")
            .filter(|c| {
                profiles
                    .get(&c.appid.to_string())
                    .is_some_and(|p| p.qualifies(include_partial))
            })
            .map(|c| c.name.as_str())
            .collect();
        picks.sort_unstable_by_key(|n| n.to_lowercase());
        println!(
            "\nSBO: Controller Friendly (include_partial = {include_partial}): {} games",
            picks.len()
        );
        for name in picks.iter().take(8) {
            println!("  - {name}");
        }
    }
}
