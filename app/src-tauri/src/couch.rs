//! Couch / TV play-style profiling.
//!
//! Derives "can I play this from the sofa with a gamepad?" facts for each owned
//! game from the **already cached** Steam store categories — no extra network
//! calls, no new cache file. Steam publishes these as category descriptions on
//! `appdetails`, and `StoreDetails::categories` already keeps them verbatim.
//!
//! Matching is exact (case-insensitive) rather than substring: "Tracked
//! Controller Support" is a *VR* controller category and must never count as
//! gamepad support.

use crate::steam_api::StoreDetails;
use serde::Serialize;
use std::collections::HashMap;

// -- Steam category descriptions we care about --

const CAT_FULL_CONTROLLER: &str = "full controller support";
const CAT_PARTIAL_CONTROLLER: &str = "partial controller support";
const CAT_REMOTE_PLAY_TV: &str = "remote play on tv";
const CAT_REMOTE_PLAY_TOGETHER: &str = "remote play together";
const CAT_VR_ONLY: &str = "vr only";
const SPLIT_SCREEN_CATS: [&str; 3] = [
    "shared/split screen",
    "shared/split screen co-op",
    "shared/split screen pvp",
];

/// Gamepad support level as reported by Steam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ControllerSupport {
    /// "Full controller support" — menus included, no mouse needed.
    Full,
    /// "Partial Controller Support" — expect to reach for the keyboard.
    Partial,
    /// Store data says neither — keyboard/mouse game.
    None,
    /// No store details cached for this app yet.
    Unknown,
}

/// Couch-readiness facts for one owned game.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CouchProfile {
    pub appid: u64,
    pub controller: ControllerSupport,
    /// Streams to a TV through Steam Remote Play / Steam Link.
    pub remote_play_tv: bool,
    /// Local multiplayer on one screen.
    pub split_screen: bool,
    /// Remote Play Together (invite friends to a local-only game).
    pub remote_play_together: bool,
    /// VR headset required — never a sofa game.
    pub vr_only: bool,
    /// Full gamepad support and not VR-only: start it with a gamepad and never
    /// touch a keyboard.
    pub tv_ready: bool,
    /// Sort weight, higher = better on a TV.
    pub score: i32,
    /// Short human-readable chips for the UI, strongest first.
    pub badges: Vec<String>,
}

impl CouchProfile {
    /// Does this game belong in the couch collection?
    /// `include_partial` relaxes the bar to partial gamepad support.
    pub fn qualifies(&self, include_partial: bool) -> bool {
        if self.vr_only {
            return false;
        }
        match self.controller {
            ControllerSupport::Full => true,
            ControllerSupport::Partial => include_partial,
            ControllerSupport::None | ControllerSupport::Unknown => false,
        }
    }
}

fn has_category(categories: &[String], want: &str) -> bool {
    categories.iter().any(|c| c.trim().eq_ignore_ascii_case(want))
}

/// Build the couch profile for a single app from its cached store details.
/// `None` details (never fetched, or the store call failed) yields `Unknown`.
pub fn profile_for(appid: u64, details: Option<&StoreDetails>) -> CouchProfile {
    let Some(details) = details else {
        return CouchProfile {
            appid,
            controller: ControllerSupport::Unknown,
            remote_play_tv: false,
            split_screen: false,
            remote_play_together: false,
            vr_only: false,
            tv_ready: false,
            score: 0,
            badges: Vec::new(),
        };
    };

    let cats = &details.categories;

    let controller = if has_category(cats, CAT_FULL_CONTROLLER) {
        ControllerSupport::Full
    } else if has_category(cats, CAT_PARTIAL_CONTROLLER) {
        ControllerSupport::Partial
    } else if cats.is_empty() {
        // Store entry exists but carries no categories at all — treat as
        // unknown rather than asserting "no gamepad".
        ControllerSupport::Unknown
    } else {
        ControllerSupport::None
    };

    let remote_play_tv = has_category(cats, CAT_REMOTE_PLAY_TV);
    let remote_play_together = has_category(cats, CAT_REMOTE_PLAY_TOGETHER);
    let split_screen = SPLIT_SCREEN_CATS
        .iter()
        .any(|want| has_category(cats, want));
    let vr_only = has_category(cats, CAT_VR_ONLY);

    let tv_ready = matches!(controller, ControllerSupport::Full) && !vr_only;

    let mut score = match controller {
        ControllerSupport::Full => 100,
        ControllerSupport::Partial => 40,
        ControllerSupport::None => 0,
        ControllerSupport::Unknown => 0,
    };
    if remote_play_tv {
        score += 15;
    }
    if split_screen {
        score += 10;
    }
    if remote_play_together {
        score += 5;
    }
    if vr_only {
        score = -1000;
    }

    let mut badges = Vec::new();
    match controller {
        ControllerSupport::Full => badges.push("Full controller".to_string()),
        ControllerSupport::Partial => badges.push("Partial controller".to_string()),
        ControllerSupport::None => badges.push("Keyboard & mouse".to_string()),
        ControllerSupport::Unknown => {}
    }
    if split_screen {
        badges.push("Split screen".to_string());
    }
    if remote_play_tv {
        badges.push("Remote Play on TV".to_string());
    }
    if remote_play_together {
        badges.push("Remote Play Together".to_string());
    }
    if vr_only {
        badges.push("VR only".to_string());
    }

    CouchProfile {
        appid,
        controller,
        remote_play_tv,
        split_screen,
        remote_play_together,
        vr_only,
        tv_ready,
        score,
        badges,
    }
}

/// Profile every given app id. Keys are stringified app ids to match the other
/// caches the frontend consumes.
pub fn build_profiles(
    appids: &[u64],
    store_cache: &HashMap<String, StoreDetails>,
) -> HashMap<String, CouchProfile> {
    appids
        .iter()
        .map(|&appid| {
            let key = appid.to_string();
            let profile = profile_for(appid, store_cache.get(&key));
            (key, profile)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details(categories: &[&str]) -> StoreDetails {
        StoreDetails {
            app_type: "game".into(),
            genres: Vec::new(),
            categories: categories.iter().map(|s| s.to_string()).collect(),
            short_description: None,
            metacritic: None,
            recommendations: None,
            developers: Vec::new(),
            release_date: None,
        }
    }

    #[test]
    fn full_controller_is_tv_ready() {
        let p = profile_for(1, Some(&details(&["Single-player", "Full controller support"])));
        assert_eq!(p.controller, ControllerSupport::Full);
        assert!(p.tv_ready);
        assert!(p.qualifies(false));
        assert_eq!(p.badges[0], "Full controller");
    }

    #[test]
    fn partial_controller_needs_opt_in() {
        let p = profile_for(2, Some(&details(&["Partial Controller Support"])));
        assert_eq!(p.controller, ControllerSupport::Partial);
        assert!(!p.tv_ready);
        assert!(!p.qualifies(false));
        assert!(p.qualifies(true));
    }

    #[test]
    fn no_controller_category_means_keyboard_and_mouse() {
        let p = profile_for(3, Some(&details(&["Single-player", "Steam Achievements"])));
        assert_eq!(p.controller, ControllerSupport::None);
        assert!(!p.qualifies(true));
    }

    #[test]
    fn tracked_controller_support_is_not_gamepad_support() {
        // VR controllers — the substring "Controller Support" must not match.
        let p = profile_for(4, Some(&details(&["Tracked Controller Support", "VR Only"])));
        assert_eq!(p.controller, ControllerSupport::None);
        assert!(p.vr_only);
        assert!(!p.qualifies(true));
    }

    #[test]
    fn vr_only_never_qualifies_even_with_full_support() {
        let p = profile_for(5, Some(&details(&["Full controller support", "VR Only"])));
        assert_eq!(p.controller, ControllerSupport::Full);
        assert!(!p.tv_ready);
        assert!(!p.qualifies(true));
        assert!(p.score < 0);
    }

    #[test]
    fn missing_or_empty_store_details_are_unknown() {
        let missing = profile_for(6, None);
        assert_eq!(missing.controller, ControllerSupport::Unknown);
        assert!(!missing.qualifies(true));
        assert!(missing.badges.is_empty());

        let empty = profile_for(7, Some(&details(&[])));
        assert_eq!(empty.controller, ControllerSupport::Unknown);
    }

    #[test]
    fn couch_extras_are_detected_and_scored() {
        let p = profile_for(
            8,
            Some(&details(&[
                "Full controller support",
                "Shared/Split Screen Co-op",
                "Remote Play on TV",
                "Remote Play Together",
            ])),
        );
        assert!(p.split_screen);
        assert!(p.remote_play_tv);
        assert!(p.remote_play_together);
        assert_eq!(p.score, 130);
        assert!(p.badges.contains(&"Split screen".to_string()));
    }

    #[test]
    fn categories_match_case_insensitively() {
        let p = profile_for(9, Some(&details(&["FULL CONTROLLER SUPPORT"])));
        assert_eq!(p.controller, ControllerSupport::Full);
    }

    #[test]
    fn build_profiles_covers_every_requested_app() {
        let mut store = HashMap::new();
        store.insert("10".to_string(), details(&["Full controller support"]));
        let profiles = build_profiles(&[10, 11], &store);
        assert_eq!(profiles.len(), 2);
        assert!(profiles["10"].tv_ready);
        assert_eq!(profiles["11"].controller, ControllerSupport::Unknown);
    }
}
