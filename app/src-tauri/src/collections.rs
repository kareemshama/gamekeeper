use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const STEAM_BASE: &str = "C:/Program Files (x86)/Steam";

/// Collection names written to Steam.
///
/// These are plain names, not the old `SBO:`-prefixed ones ("SBO" was the
/// project's first name, Steam Backlog Organizer). Gamekeeper now **takes over
/// collections with these names** instead of writing a parallel prefixed set —
/// one list per category, no duplicates. Anything named differently is left
/// alone.
pub fn collection_names() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("COMPLETED", "Completed"),
        ("IN_PROGRESS", "In Progress"),
        ("ENDLESS", "Endless/Multiplayer"),
        ("NOT_A_GAME", "Not a Game"),
    ])
}

/// The four classification collections, in a stable display order.
pub const CATEGORY_ORDER: [&str; 4] = ["COMPLETED", "IN_PROGRESS", "ENDLESS", "NOT_A_GAME"];

/// Opt-in play-style collection: gamepad-friendly games for a TV / couch setup.
pub const COUCH_COLLECTION_NAME: &str = "Controller Friendly";

/// Collections written by older versions under the `SBO:` prefix. Removed on
/// the next write so the rename doesn't leave duplicates behind.
pub const LEGACY_COLLECTION_NAMES: [&str; 5] = [
    "SBO: Completed",
    "SBO: In Progress",
    "SBO: Endless",
    "SBO: Not a Game",
    "SBO: Controller Friendly",
];

/// Check if Steam is currently running (Windows).
pub fn is_steam_running() -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq steam.exe"])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
                stdout.contains("steam.exe")
            }
            Err(_) => false,
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let output = std::process::Command::new("pgrep")
            .arg("-x")
            .arg("steam")
            .output();
        matches!(output, Ok(out) if out.status.success())
    }
}

/// Steam account info found in userdata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SteamAccount {
    pub id: String,
    pub path: String,
}

/// List all Steam userdata account directories.
pub fn get_steam_accounts() -> Vec<SteamAccount> {
    let userdata = PathBuf::from(STEAM_BASE).join("userdata");
    if !userdata.exists() {
        return Vec::new();
    }

    let mut accounts = Vec::new();
    if let Ok(entries) = fs::read_dir(&userdata) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    accounts.push(SteamAccount {
                        id: name.to_string(),
                        path: entry.path().to_string_lossy().to_string(),
                    });
                }
            }
        }
    }

    accounts
}

/// Path to the cloud storage JSON for a given userdata account.
fn cloud_storage_path(userdata_path: &Path) -> PathBuf {
    userdata_path
        .join("config")
        .join("cloudstorage")
        .join("cloud-storage-namespace-1.json")
}

/// Load the cloud storage JSON that contains Steam collections.
pub fn load_steam_collections(userdata_path: &Path) -> Result<(Vec<Value>, PathBuf), String> {
    let path = cloud_storage_path(userdata_path);
    if !path.exists() {
        return Ok((Vec::new(), path));
    }

    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Could not read Steam collections file: {e}"))?;
    let parsed: Vec<Value> =
        serde_json::from_str(&data).map_err(|e| format!("Could not parse collections: {e}"))?;

    Ok((parsed, path))
}

/// Existing collection info parsed from cloud data.
struct ExistingCollection {
    key: String,
    id: String,
    /// App ids currently in the collection, in Steam's order.
    added: Vec<u64>,
}

/// Extract user collections from cloud storage data.
fn get_existing_collections(cloud_data: &[Value]) -> HashMap<String, ExistingCollection> {
    let mut collections = HashMap::new();

    for entry in cloud_data {
        let arr = match entry.as_array() {
            Some(a) if a.len() >= 2 => a,
            _ => continue,
        };

        let key = match arr[0].as_str() {
            Some(k) if k.starts_with("user-collections.") => k,
            _ => continue,
        };

        let meta = match arr[1].as_object() {
            Some(m) => m,
            None => continue,
        };

        // Skip deleted entries
        if meta
            .get("is_deleted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }

        let value_str = match meta.get("value").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };

        let value: Value = match serde_json::from_str(value_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let name = match value.get("name").and_then(|n| n.as_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        let id = value
            .get("id")
            .and_then(|i| i.as_str())
            .unwrap_or("")
            .to_string();
        let added: Vec<u64> = value
            .get("added")
            .and_then(|a| a.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
            .unwrap_or_default();
        collections.insert(
            name,
            ExistingCollection {
                key: key.to_string(),
                id,
                added,
            },
        );
    }

    collections
}

/// Current membership of every live collection, keyed by display name.
///
/// Used to compute "add new games only" writes: a game already filed in one of
/// the managed collections is left where the user put it.
pub fn existing_collection_members(cloud_data: &[Value]) -> HashMap<String, Vec<u64>> {
    get_existing_collections(cloud_data)
        .into_iter()
        .map(|(name, coll)| (name, coll.added))
        .collect()
}

/// Generate a random collection ID in Steam's format: uc-XXXXXXXXXXXX
fn generate_collection_id() -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut rng = rand::rng();
    let random_part: String = (0..12)
        .map(|_| {
            let idx = rng.random_range(0..CHARS.len());
            CHARS[idx] as char
        })
        .collect();
    format!("uc-{random_part}")
}

/// Find the highest version number in cloud data and return next.
fn get_next_version(cloud_data: &[Value]) -> u64 {
    let mut max_version: u64 = 0;
    for entry in cloud_data {
        if let Some(arr) = entry.as_array() {
            if arr.len() >= 2 {
                if let Some(v_str) = arr[1].get("version").and_then(|v| v.as_str()) {
                    if let Ok(v) = v_str.parse::<u64>() {
                        if v > max_version {
                            max_version = v;
                        }
                    }
                }
            }
        }
    }
    max_version + 1
}

/// Write classification results as Steam collections.
///
/// categories: { "COMPLETED": [appid1, appid2, ...], ... }
pub fn write_collections_to_steam(
    cloud_data: &mut Vec<Value>,
    cloud_path: &Path,
    categories: &HashMap<String, Vec<u64>>,
) -> Result<(), String> {
    let coll_names = collection_names();
    let sets: Vec<(String, Vec<u64>)> = CATEGORY_ORDER
        .iter()
        .map(|key| {
            let name = coll_names[key].to_string();
            let ids = categories.get(*key).cloned().unwrap_or_default();
            (name, ids)
        })
        .collect();
    write_collection_sets(cloud_data, cloud_path, &sets, &[]).map(|_| ())
}

/// Write an explicit list of named collections, and remove named leftovers.
///
/// Each entry in `sets` is `(display name, app ids)`. Collections are created
/// when absent and replaced when present; any collection **not** named here is
/// left exactly as Steam has it, so an opt-out never silently empties a list the
/// user keeps.
///
/// `remove` names collections to delete (used to clear the old `SBO:`-prefixed
/// set after the rename). A name present in both `sets` and `remove` is written,
/// never deleted. Returns the names actually deleted.
pub fn write_collection_sets(
    cloud_data: &mut Vec<Value>,
    cloud_path: &Path,
    sets: &[(String, Vec<u64>)],
    remove: &[&str],
) -> Result<Vec<String>, String> {
    let existing = get_existing_collections(cloud_data);
    let mut version = get_next_version(cloud_data);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Time error: {e}"))?
        .as_secs();
    let mut modified_keys = Vec::new();

    for (display_name, ids) in sets {
        let app_ids: Vec<Value> = ids.iter().map(|id| Value::from(*id)).collect();

        if let Some(coll) = existing.get(display_name) {
            // Update existing collection
            let new_value = serde_json::json!({
                "id": coll.id,
                "name": display_name,
                "added": app_ids,
                "removed": [],
            });

            // Find and update the entry in cloud_data
            for entry in cloud_data.iter_mut() {
                if let Some(arr) = entry.as_array_mut() {
                    if arr.len() >= 2 {
                        if let Some(k) = arr[0].as_str() {
                            if k == coll.key {
                                if let Some(meta) = arr[1].as_object_mut() {
                                    meta.insert(
                                        "value".into(),
                                        Value::String(new_value.to_string()),
                                    );
                                    meta.insert(
                                        "timestamp".into(),
                                        Value::from(timestamp),
                                    );
                                    meta.insert(
                                        "version".into(),
                                        Value::String(version.to_string()),
                                    );
                                }
                                modified_keys.push(coll.key.clone());
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            // Create new collection
            let coll_id = generate_collection_id();
            let coll_key = format!("user-collections.{coll_id}");
            let new_value = serde_json::json!({
                "id": coll_id,
                "name": display_name,
                "added": app_ids,
                "removed": [],
            });

            cloud_data.push(serde_json::json!([
                coll_key.clone(),
                {
                    "key": coll_key.clone(),
                    "timestamp": timestamp,
                    "value": new_value.to_string(),
                    "version": version.to_string(),
                    // Same conflict handling Steam itself stamps on user
                    // collections — keeps cloud merges behaving normally.
                    "conflictResolutionMethod": "custom",
                    "strMethodId": "union-collections",
                }
            ]));
            modified_keys.push(coll_key);
        }

        version += 1;
    }

    // Remove leftovers (the old SBO:-prefixed set) by marking them deleted the
    // way Steam does: key + timestamp + is_deleted, with the value dropped.
    let mut removed = Vec::new();
    for name in remove {
        if sets.iter().any(|(written, _)| written == name) {
            continue;
        }
        let Some(coll) = existing.get(*name) else {
            continue;
        };
        for entry in cloud_data.iter_mut() {
            let Some(arr) = entry.as_array_mut() else {
                continue;
            };
            if arr.len() < 2 || arr[0].as_str() != Some(coll.key.as_str()) {
                continue;
            }
            arr[1] = serde_json::json!({
                "key": coll.key.clone(),
                "timestamp": timestamp,
                "is_deleted": true,
                "version": version.to_string(),
            });
            modified_keys.push(coll.key.clone());
            removed.push((*name).to_string());
            version += 1;
            break;
        }
    }

    // Write the collection data
    let json_str = serde_json::to_string(cloud_data)
        .map_err(|e| format!("Failed to serialize collections: {e}"))?;
    fs::write(cloud_path, &json_str)
        .map_err(|e| format!("Could not write collections file: {e}"))?;

    // Update modified keys file so Steam syncs them
    let modified_path = cloud_path.with_file_name("cloud-storage-namespace-1.modified.json");
    let mut all_modified: Vec<String> = if modified_path.exists() {
        fs::read_to_string(&modified_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    for key in &modified_keys {
        if !all_modified.contains(key) {
            all_modified.push(key.clone());
        }
    }
    let modified_json = serde_json::to_string(&all_modified)
        .map_err(|e| format!("Failed to serialize modified keys: {e}"))?;
    fs::write(&modified_path, &modified_json)
        .map_err(|e| format!("Could not write modified keys file: {e}"))?;

    // Update namespace version so Steam knows local state is newer
    let namespaces_path = cloud_path.with_file_name("cloud-storage-namespaces.json");
    if namespaces_path.exists() {
        if let Ok(data) = fs::read_to_string(&namespaces_path) {
            if let Ok(mut namespaces) = serde_json::from_str::<Vec<Value>>(&data) {
                for ns in &mut namespaces {
                    if let Some(arr) = ns.as_array_mut() {
                        if arr.len() >= 2 {
                            if arr[0].as_u64() == Some(1) {
                                arr[1] = Value::String(version.to_string());
                                break;
                            }
                        }
                    }
                }
                if let Ok(ns_json) = serde_json::to_string(&namespaces) {
                    let _ = fs::write(&namespaces_path, &ns_json);
                }
            }
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collection_entry(id: &str, name: &str, added: &[u64]) -> Value {
        let key = format!("user-collections.{id}");
        serde_json::json!([
            key.clone(),
            {
                "key": key,
                "timestamp": 1_700_000_000u64,
                "value": serde_json::json!({
                    "id": id,
                    "name": name,
                    "added": added,
                    "removed": [],
                }).to_string(),
                "version": "100",
            }
        ])
    }

    fn live_names(cloud_data: &[Value]) -> Vec<String> {
        let mut names: Vec<String> = get_existing_collections(cloud_data)
            .into_keys()
            .collect();
        names.sort();
        names
    }

    #[test]
    fn category_names_are_unprefixed() {
        let names = collection_names();
        assert_eq!(names["COMPLETED"], "Completed");
        assert_eq!(names["ENDLESS"], "Endless/Multiplayer");
        assert_eq!(COUCH_COLLECTION_NAME, "Controller Friendly");
        // The rename must not quietly reintroduce the old prefix.
        assert!(names.values().all(|n| !n.starts_with("SBO:")));
    }

    #[test]
    fn write_takes_over_existing_collection_and_clears_legacy() {
        let dir = std::env::temp_dir().join(format!(
            "gk-collections-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cloud-storage-namespace-1.json");

        let mut cloud_data = vec![
            // The user's own collection the tool now owns
            collection_entry("uc-existing0001", "Completed", &[1, 2, 3]),
            // A leftover from the prefixed era
            collection_entry("uc-legacy000001", "SBO: Completed", &[1, 2, 3]),
            // Untouchable: a hand-made collection we never name
            collection_entry("uc-handmade0001", "PLAYSTATION", &[42]),
        ];

        let sets = vec![("Completed".to_string(), vec![7u64, 8])];
        let removed =
            write_collection_sets(&mut cloud_data, &path, &sets, &LEGACY_COLLECTION_NAMES).unwrap();

        assert_eq!(removed, vec!["SBO: Completed".to_string()]);
        assert_eq!(
            live_names(&cloud_data),
            vec!["Completed".to_string(), "PLAYSTATION".to_string()]
        );

        // Updated in place — same collection id, new contents
        let existing = get_existing_collections(&cloud_data);
        assert_eq!(existing["Completed"].id, "uc-existing0001");
        let value: Value = serde_json::from_str(
            cloud_data
                .iter()
                .find(|e| e[0].as_str() == Some("user-collections.uc-existing0001"))
                .unwrap()[1]["value"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["added"], serde_json::json!([7, 8]));

        // The hand-made collection is byte-for-byte untouched
        let handmade = cloud_data
            .iter()
            .find(|e| e[0].as_str() == Some("user-collections.uc-handmade0001"))
            .unwrap();
        assert_eq!(handmade[1]["version"].as_str(), Some("100"));

        // Deletion uses Steam's shape: no value, is_deleted set
        let deleted = cloud_data
            .iter()
            .find(|e| e[0].as_str() == Some("user-collections.uc-legacy000001"))
            .unwrap();
        assert_eq!(deleted[1]["is_deleted"].as_bool(), Some(true));
        assert!(deleted[1].get("value").is_none());

        // Steam needs every touched key listed for sync
        let modified: Vec<String> = serde_json::from_str(
            &fs::read_to_string(dir.join("cloud-storage-namespace-1.modified.json")).unwrap(),
        )
        .unwrap();
        assert!(modified.contains(&"user-collections.uc-existing0001".to_string()));
        assert!(modified.contains(&"user-collections.uc-legacy000001".to_string()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_name_in_both_sets_and_remove_is_written_not_deleted() {
        let dir = std::env::temp_dir().join(format!(
            "gk-collections-test-keep-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cloud-storage-namespace-1.json");

        let mut cloud_data = vec![collection_entry("uc-both00000001", "SBO: Completed", &[1])];
        let sets = vec![("SBO: Completed".to_string(), vec![9u64])];
        let removed =
            write_collection_sets(&mut cloud_data, &path, &sets, &LEGACY_COLLECTION_NAMES).unwrap();

        assert!(removed.is_empty());
        assert_eq!(live_names(&cloud_data), vec!["SBO: Completed".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }
}
