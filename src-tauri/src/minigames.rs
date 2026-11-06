//! The minigames' names, which the game only shows as pictures: those of the
//! Minigame Picker (wp-minigame-picker/js/data.js), in English as the European and
//! the American discs write them. A package names its own, or renames those it
//! replaces, with `minigame_names` in its manifest.

use crate::mods::Installed;
use std::collections::BTreeMap;

/// (id, European name, American name).
const NAMES: [(i32, &str, &str); 79] = [
    (100, "Commuter Count", "Commuter Count"),
    (101, "Timber Topple", "Timber Topple"),
    (102, "Delivery Duel", "Delivery Duel"),
    (104, "Roll to the Goal", "Roll to the Goal"),
    (106, "Channel Changers", "Channel Changers"),
    (107, "Spring Time", "Spring Time"),
    (108, "Shepherd Scramble", "Jangle Wranglers"),
    (109, "Poppin' Pilots", "Poppin' Pilots"),
    (110, "Flag Footrace", "Flag Footrace"),
    (111, "Pop-Up Bandits", "Pop-Up Bandits"),
    (112, "Fruit Focus", "Fruit Focus"),
    (200, "Flycycle Team", "Flycycle Team"),
    (201, "Fishin' Buddies", "Fishin' Buddies"),
    (202, "Bumper Crops", "Bumper Crops"),
    (203, "Robot Factory", "Robot Factory"),
    (204, "Torchlit Terror", "Flashlight Frights"),
    (205, "Basket Bonanza", "Basket Bonanza"),
    (206, "Meet and Greet", "Meet and Greet"),
    (208, "Paddle Pals", "Paddle Pals"),
    (209, "Pop Coaster", "Pop Coaster"),
    (210, "Tippy Traverse", "Tippy Traverse"),
    (211, "Pump Cart Panic", "Pump Cart Panic"),
    (212, "Sheep Crossing", "Sheep Crossing"),
    (213, "Banana Blockade", "Banana Blockade"),
    (214, "Spring Ringers", "Spring Ringers"),
    (215, "Can Poppers", "Can Poppers"),
    (216, "Hidden Charms", "Clover Hunt"),
    (218, "Rodent Rundown", "Rodent Rundown"),
    (219, "Skipping Squad", "Jump Rope Jam"),
    (221, "Bobsleigh Highway", "Bobsled Highway"),
    (222, "Colour Coordination", "Color Coordination"),
    (223, "Teamwork Temple", "Teamwork Temple"),
    (224, "Blastronauts", "Blastronauts"),
    (300, "Flying Fruit", "Flying Fruit"),
    (301, "Lumber Whacks", "Lumber Whacks"),
    (302, "Splash Bash", "Splash Bash"),
    (303, "Poserobics", "Poserobics"),
    (304, "Hide and Peek", "Hide-and-Peek"),
    (400, "Derby Dash", "Derby Dash"),
    (401, "Smile Snap", "Smile Snap"),
    (402, "Chop Chops", "Chop Chops"),
    (403, "Walk-Off", "Walk-Off"),
    (404, "Shifty Gifts", "Shifty Gifts"),
    (405, "Hurdle Hover", "Flap Hurdles"),
    (406, "Jumbo Jump", "Jumbo Jump"),
    (407, "Lunar Landers", "Moon Landing"),
    (408, "Popgun Posse", "Popgun Posse"),
    (410, "Friendly Face-Off", "Friendly Face-Off"),
    (411, "Flag Fracas", "Flag Fracas"),
    (412, "Shutterpup", "Shutterpup"),
    (413, "Lucky Launch", "Lucky Launch"),
    (414, "Saucer Snap", "Saucer Snap"),
    (415, "Maze Daze", "Maze Daze"),
    (416, "Quicker Chipper", "Quicker Chipper"),
    (418, "Barrel Daredevil", "Barrel Daredevil"),
    (419, "Face Flip", "Face Flip"),
    (420, "Goal Getters", "Goal Getters"),
    (421, "Stop Watchers", "Stop Watchers"),
    (422, "Chopper Hoppers", "Chopper Hoppers"),
    (423, "Puzzle Pick-Up", "Puzzle Pick-Up"),
    (424, "Feathered Frenzy", "Feathered Frenzy"),
    (426, "Follow Your Face", "Follow Your Face"),
    (427, "Strategy Steps", "Strategy Steps"),
    (428, "Tropical Punch", "Tropical Punch"),
    (430, "Zombie Tag", "Zombie Tag"),
    (431, "Pearl Plunder", "Pearl Plunder"),
    (432, "Hammer Heads", "Hammer Heads"),
    (433, "Ball Brawl", "Crash Balls"),
    (434, "Lofty Leap", "Rope Sling"),
    (436, "Space Brawl", "Space Brawl"),
    (437, "Dicey Descent", "Dicey Descent"),
    (438, "Space Race", "Cosmic Capers"),
    (439, "Cry Babies", "Cry Babies"),
    (440, "Ram Jam", "Ram Jam"),
    (441, "Spotlight Fight", "Spotlight Fight"),
    (442, "Chin-Up Champ", "Chin-Up Champ"),
    (443, "Risky Railway", "Risky Railroad"),
    (445, "Balloon Buggies", "Balloon Buggies"),
    (446, "Back Attack", "Back Attack"),
];

/// Minigames that play another one's REL (wii_party_notes.txt): 103 is 418's, 417 is 111's.
const SAME_AS: [(i32, i32); 2] = [(103, 418), (417, 111)];

/// Every minigame's name, by id, for the disc `game_id` with `packages` (in load order).
pub fn names(game_id: &str, packages: &[Installed]) -> BTreeMap<i32, String> {
    let american = game_id.get(3..4) == Some("E");
    let mut names: BTreeMap<i32, String> = NAMES.iter().map(|&(id, eu, us)| (id, if american { us } else { eu }.to_string())).collect();
    for (id, of) in SAME_AS {
        if let Some(name) = names.get(&of).cloned() {
            names.insert(id, name);
        }
    }
    for package in packages {
        for (id, name) in &package.manifest.minigame_names {
            if let Ok(id) = id.parse() {
                names.insert(id, name.to_string());
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_by_region() {
        assert_eq!(names("SUPP01", &[])[&204], "Torchlit Terror");
        assert_eq!(names("SUPE01", &[])[&204], "Flashlight Frights");
        assert_eq!(names("SUPP01", &[])[&103], names("SUPP01", &[])[&418]);
    }
}
