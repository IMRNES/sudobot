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
//! curve steepens, so `Apprentice` is a first-week badge and `IMMORTAL` is a
//! resident's. The spacing here is the design, not an artefact of the curve.
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
        level: 3,
        name: "Apprentice",
        colour: 0x7F8C8D,
    },
    LadderRung {
        level: 10,
        name: "Pathfinder",
        colour: 0x27AE60,
    },
    LadderRung {
        level: 20,
        name: "Seeker",
        colour: 0x2ECC71,
    },
    LadderRung {
        level: 40,
        name: "Visionary",
        colour: 0x16A085,
    },
    LadderRung {
        level: 80,
        name: "Voyager",
        colour: 0x1ABC9C,
    },
    LadderRung {
        level: 100,
        name: "Adept",
        colour: 0x3498DB,
    },
    LadderRung {
        level: 120,
        name: "Hacker",
        colour: 0x2980B9,
    },
    LadderRung {
        level: 240,
        name: "Mage",
        colour: 0x9B59B6,
    },
    LadderRung {
        level: 280,
        name: "Wizard",
        colour: 0x8E44AD,
    },
    LadderRung {
        level: 300,
        name: "Master",
        colour: 0xE67E22,
    },
    LadderRung {
        level: 350,
        name: "Guru",
        colour: 0xF39C12,
    },
    LadderRung {
        level: 400,
        name: "Legend",
        colour: 0xE74C3C,
    },
    LadderRung {
        level: 450,
        name: "Guardian",
        colour: 0xC0392B,
    },
    LadderRung {
        level: 500,
        name: "TITAN",
        colour: 0xD35400,
    },
    LadderRung {
        level: 600,
        name: "SAGE",
        colour: 0xF1C40F,
    },
    LadderRung {
        level: 700,
        name: "VANGUARD",
        colour: 0xFF6B35,
    },
    LadderRung {
        level: 800,
        name: "ASCENDED",
        colour: 0xFF4D6D,
    },
    LadderRung {
        level: 900,
        name: "DEITY",
        colour: 0xA855F7,
    },
    LadderRung {
        level: 1000,
        name: "MYTHIC",
        colour: 0xF43F5E,
    },
    LadderRung {
        level: 2000,
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
        (3, "Apprentice"),
        (10, "Pathfinder"),
        (20, "Seeker"),
        (40, "Visionary"),
        (80, "Voyager"),
        (100, "Adept"),
        (120, "Hacker"),
        (240, "Mage"),
        (280, "Wizard"),
        (300, "Master"),
        (350, "Guru"),
        (400, "Legend"),
        (450, "Guardian"),
        (500, "TITAN"),
        (600, "SAGE"),
        (700, "VANGUARD"),
        (800, "ASCENDED"),
        (900, "DEITY"),
        (1000, "MYTHIC"),
        (2000, "IMMORTAL"),
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
        assert_eq!(rung_for_level(3).unwrap().name, "Apprentice");
        assert_eq!(rung_for_level(9).unwrap().name, "Apprentice");
        assert_eq!(rung_for_level(1999).unwrap().name, "MYTHIC");
        assert_eq!(rung_for_level(2000).unwrap().name, "IMMORTAL");
        // Past the top rung the ladder saturates rather than running off the end.
        assert_eq!(rung_for_level(50_000).unwrap().name, "IMMORTAL");
    }

    #[test]
    fn a_negative_level_qualifies_for_nothing() {
        assert!(rung_for_level(-1).is_none());
    }

    #[test]
    fn rung_at_is_exact_while_rung_for_level_is_not() {
        assert!(rung_at(4).is_none());
        assert_eq!(rung_at(3).unwrap().name, "Apprentice");
        // Level 4 is `Apprentice` by qualification but has no rung of its own.
        assert_eq!(rung_for_level(4).unwrap().name, "Apprentice");
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
