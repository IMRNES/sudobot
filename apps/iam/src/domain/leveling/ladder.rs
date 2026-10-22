//! The cosmetic ladder: one title per level, and the order they are earned in.
//!
//! These are the guild's trophy roles. They grant no Discord permission at all —
//! [`LadderRung::permissions`] is what makes that a property of the data rather
//! than a promise in a README — so holding `IMMORTAL` is a title, not an
//! escalation.
//!
//! The table lives in the domain because it is the one description of the ladder
//! that both surfaces need. The bot creates the Discord roles from it and the
//! dashboard renders the same names, so a rung cannot be called one thing on a
//! member's profile and another in the reward list.
//!
//! Levels are not evenly spaced and that is the point: the gaps widen as the
//! curve steepens, so `Apprentice` is a first-month badge and `IMMORTAL` is a
//! resident's. The spacing here is the design, not an artefact of the curve.
//!
//! The thresholds were tightened in 2026-10: the original ladder reached
//! `IMMORTAL` at level 2000, which the Mrn curve crosses in a few hundred
//! messages. The gaps now widen roughly geometrically, so the middle rungs are
//! measured in thousands of messages rather than hundreds and the top rung is
//! something a member works toward for months rather than days.
//!
//! The first rung is level 0, which is where everybody starts. A level-0 reward
//! therefore never fires from a level *change* — nobody levels up into it — and is
//! granted by reconciliation instead: on join, and by the ladder provisioning and
//! sync. That is deliberate, because the alternative is a role the entire guild
//! earns at the moment it is configured.

/// One title in the ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LadderRung {
    /// The level that earns this title. Never negative.
    pub level: i32,
    /// The Discord role name, which is also the display name.
    pub name: &'static str,
    /// The role colour, as `0xRRGGBB`.
    ///
    /// Cosmetic, but not arbitrary: the ladder walks grey → green → blue →
    /// purple → ember → gold, so a member's colour alone places them roughly on
    /// the climb.
    pub colour: u32,
}

impl LadderRung {
    /// The permissions every rung grants: none.
    ///
    /// A method rather than a field so a rung cannot be built with a permission
    /// by mistake, and so the intent is legible at the call site that creates the
    /// Discord role.
    pub fn permissions() -> u64 {
        0
    }

    /// The badge artwork for this rung, as a path relative to the dashboard's
    /// static root.
    ///
    /// Derived from the name rather than stored per rung: the badges are one
    /// generated file per title, `web/public/rank-icons/<slug>.png`, and a name is
    /// already unique (the reward table indexes `(guild_id, level)` and
    /// [`is_ladder_name`] matches exactly). A field would be a second place to
    /// forget when a rung is added.
    ///
    /// Discord wants a URL, and the dashboard is the one origin that serves these
    /// publicly, so the bot and the UI read the same asset. The file name is
    /// lowercase-hyphenated because that is what the generator writes.
    pub fn icon_path(&self) -> String {
        format!("rank-icons/{}.png", slug(self.name))
    }
}

/// A role name as a file slug: lowercase, runs of non-alphanumerics collapsed to
/// one hyphen.
///
/// `TITAN`, `SAGE` and the other all-caps rungs must produce the same file as a
/// mixed-case one would, or adding a capitalised title would silently look for an
/// asset nobody generated.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out
}

/// The whole ladder, ascending by level.
///
/// Sorted and unique, which [`ladder_is_well_formed`] asserts at test time: a
/// duplicate level would make "which title does this member hold" ambiguous, and
/// the reward table has a unique index on `(guild_id, level)` that would reject
/// the second one at runtime anyway.
pub const LADDER: [LadderRung; 21] = [
    LadderRung {
        level: 0,
        name: "Neophyte",
        colour: 0x95A5A6,
    },
    LadderRung {
        level: 10,
        name: "Apprentice",
        colour: 0x7F8C8D,
    },
    LadderRung {
        level: 35,
        name: "Pathfinder",
        colour: 0x27AE60,
    },
    LadderRung {
        level: 75,
        name: "Seeker",
        colour: 0x2ECC71,
    },
    LadderRung {
        level: 150,
        name: "Visionary",
        colour: 0x16A085,
    },
    LadderRung {
        level: 300,
        name: "Voyager",
        colour: 0x1ABC9C,
    },
    LadderRung {
        level: 450,
        name: "Adept",
        colour: 0x3498DB,
    },
    LadderRung {
        level: 600,
        name: "Hacker",
        colour: 0x2980B9,
    },
    LadderRung {
        level: 900,
        name: "Mage",
        colour: 0x9B59B6,
    },
    LadderRung {
        level: 1200,
        name: "Wizard",
        colour: 0x8E44AD,
    },
    LadderRung {
        level: 1500,
        name: "Master",
        colour: 0xE67E22,
    },
    LadderRung {
        level: 1800,
        name: "Guru",
        colour: 0xF39C12,
    },
    LadderRung {
        level: 2200,
        name: "Legend",
        colour: 0xE74C3C,
    },
    LadderRung {
        level: 2700,
        name: "Guardian",
        colour: 0xC0392B,
    },
    LadderRung {
        level: 3300,
        name: "TITAN",
        colour: 0xD35400,
    },
    LadderRung {
        level: 4000,
        name: "SAGE",
        colour: 0xF1C40F,
    },
    LadderRung {
        level: 4800,
        name: "VANGUARD",
        colour: 0xFF6B35,
    },
    LadderRung {
        level: 5700,
        name: "ASCENDED",
        colour: 0xFF4D6D,
    },
    LadderRung {
        level: 6700,
        name: "DEITY",
        colour: 0xA855F7,
    },
    LadderRung {
        level: 7800,
        name: "MYTHIC",
        colour: 0xF43F5E,
    },
    LadderRung {
        level: 10000,
        name: "IMMORTAL",
        colour: 0xFFD700,
    },
];

/// The rung earned by exactly `level`, if the ladder has one there.
pub fn rung_at(level: i32) -> Option<&'static LadderRung> {
    LADDER.iter().find(|rung| rung.level == level)
}

/// The highest rung `level` qualifies for.
///
/// This is the question a member's profile actually asks — level 7 holds
/// `Apprentice`, because that is the last rung at or below 7. `None` only for a
/// negative level, which the ladder does not describe.
pub fn rung_for_level(level: i32) -> Option<&'static LadderRung> {
    LADDER.iter().rfind(|rung| rung.level <= level)
}

/// Whether a role name is one of ours, so provisioning can recognise its own
/// roles across a restart instead of creating a second copy of the ladder.
pub fn is_ladder_name(name: &str) -> bool {
    LADDER.iter().any(|rung| rung.name == name)
}

/// The ladder in the order a Discord role list should read, highest first.
///
/// Discord renders higher positions further up, and a ladder that reads top-down
/// as `IMMORTAL … Neophyte` is the one people expect.
pub fn descending() -> impl DoubleEndedIterator<Item = &'static LadderRung> {
    LADDER.iter().rev()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The ladder as specified, so a rung edited by accident fails here rather
    /// than silently changing what the live guild's members are called.
    const EXPECTED: [(i32, &str); 21] = [
        (0, "Neophyte"),
        (10, "Apprentice"),
        (35, "Pathfinder"),
        (75, "Seeker"),
        (150, "Visionary"),
        (300, "Voyager"),
        (450, "Adept"),
        (600, "Hacker"),
        (900, "Mage"),
        (1200, "Wizard"),
        (1500, "Master"),
        (1800, "Guru"),
        (2200, "Legend"),
        (2700, "Guardian"),
        (3300, "TITAN"),
        (4000, "SAGE"),
        (4800, "VANGUARD"),
        (5700, "ASCENDED"),
        (6700, "DEITY"),
        (7800, "MYTHIC"),
        (10000, "IMMORTAL"),
    ];

    #[test]
    fn ladder_is_well_formed() {
        assert_eq!(LADDER.len(), EXPECTED.len());

        for (position, (level, name)) in EXPECTED.iter().enumerate() {
            let rung = LADDER[position];
            assert_eq!(rung.level, *level, "rung {position} level");
            assert_eq!(rung.name, *name, "rung {position} name");
        }

        // Ascending and unique: `rung_for_level` walks in order, and the reward
        // table's unique index would reject a duplicate at runtime.
        for pair in LADDER.windows(2) {
            assert!(
                pair[0].level < pair[1].level,
                "{} ({}) must precede {} ({})",
                pair[0].name,
                pair[0].level,
                pair[1].name,
                pair[1].level
            );
        }
    }

    #[test]
    fn every_rung_is_permissionless() {
        // The whole ladder is cosmetic. A rung that granted anything would be an
        // escalation handed out automatically by a chat bot.
        assert_eq!(LadderRung::permissions(), 0);
    }

    #[test]
    fn a_level_between_rungs_holds_the_last_one_passed() {
        assert_eq!(rung_for_level(0).unwrap().name, "Neophyte");
        assert_eq!(rung_for_level(2).unwrap().name, "Neophyte");
        assert_eq!(rung_for_level(9).unwrap().name, "Neophyte");
        assert_eq!(rung_for_level(10).unwrap().name, "Apprentice");
        assert_eq!(rung_for_level(34).unwrap().name, "Apprentice");
        assert_eq!(rung_for_level(35).unwrap().name, "Pathfinder");
        assert_eq!(rung_for_level(74).unwrap().name, "Pathfinder");
        assert_eq!(rung_for_level(75).unwrap().name, "Seeker");
        assert_eq!(rung_for_level(149).unwrap().name, "Seeker");
        assert_eq!(rung_for_level(150).unwrap().name, "Visionary");
        assert_eq!(rung_for_level(299).unwrap().name, "Visionary");
        assert_eq!(rung_for_level(300).unwrap().name, "Voyager");
        assert_eq!(rung_for_level(9999).unwrap().name, "MYTHIC");
        assert_eq!(rung_for_level(10000).unwrap().name, "IMMORTAL");
        // Past the top rung the ladder saturates rather than running off the end.
        assert_eq!(rung_for_level(50_000).unwrap().name, "IMMORTAL");
    }

    #[test]
    fn a_negative_level_qualifies_for_nothing() {
        assert!(rung_for_level(-1).is_none());
    }

    #[test]
    fn rung_at_is_exact_while_rung_for_level_is_not() {
        assert!(rung_at(11).is_none());
        assert_eq!(rung_at(10).unwrap().name, "Apprentice");
        // Level 11 is `Apprentice` by qualification but has no rung of its own.
        assert_eq!(rung_for_level(11).unwrap().name, "Apprentice");
    }

    #[test]
    fn the_descending_order_is_the_climb_read_backwards() {
        let names: Vec<&str> = descending().map(|rung| rung.name).collect();
        assert_eq!(names.first().copied(), Some("IMMORTAL"));
        assert_eq!(names.last().copied(), Some("Neophyte"));
        assert_eq!(names.len(), LADDER.len());
    }

    #[test]
    fn provisioning_recognises_its_own_role_names_and_nobody_elses() {
        assert!(is_ladder_name("Neophyte"));
        assert!(is_ladder_name("IMMORTAL"));
        // The guild's existing roles must never be mistaken for a rung.
        assert!(!is_ladder_name("Admin"));
        assert!(!is_ladder_name("chmod 777"));
        assert!(!is_ladder_name("neophyte"), "matching is exact");
    }
}
