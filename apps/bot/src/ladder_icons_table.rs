// Generated table of rung -> badge file. Regenerate with
// `scripts/gen-ladder-icons-table.py` after adding a rung or artwork.
//
// A separate file because `include_bytes!` needs a string literal — the path
// cannot be computed from the rung name at compile time.
//
// Plain comments, not `//!`: this file is pulled in with `include!`, and an
// inner doc comment is only legal at the top of a module, not at the point of
// inclusion.

/// One entry per ladder rung, in ladder order.
static ICONS: [Icon; 21] = [
    Icon {
        name: "Neophyte",
        file: "neophyte.png",
        png: include_bytes!("../../../web/public/rank-icons/neophyte.png"),
    },
    Icon {
        name: "Apprentice",
        file: "apprentice.png",
        png: include_bytes!("../../../web/public/rank-icons/apprentice.png"),
    },
    Icon {
        name: "Pathfinder",
        file: "pathfinder.png",
        png: include_bytes!("../../../web/public/rank-icons/pathfinder.png"),
    },
    Icon {
        name: "Seeker",
        file: "seeker.png",
        png: include_bytes!("../../../web/public/rank-icons/seeker.png"),
    },
    Icon {
        name: "Visionary",
        file: "visionary.png",
        png: include_bytes!("../../../web/public/rank-icons/visionary.png"),
    },
    Icon {
        name: "Voyager",
        file: "voyager.png",
        png: include_bytes!("../../../web/public/rank-icons/voyager.png"),
    },
    Icon {
        name: "Adept",
        file: "adept.png",
        png: include_bytes!("../../../web/public/rank-icons/adept.png"),
    },
    Icon {
        name: "Hacker",
        file: "hacker.png",
        png: include_bytes!("../../../web/public/rank-icons/hacker.png"),
    },
    Icon {
        name: "Mage",
        file: "mage.png",
        png: include_bytes!("../../../web/public/rank-icons/mage.png"),
    },
    Icon {
        name: "Wizard",
        file: "wizard.png",
        png: include_bytes!("../../../web/public/rank-icons/wizard.png"),
    },
    Icon {
        name: "Master",
        file: "master.png",
        png: include_bytes!("../../../web/public/rank-icons/master.png"),
    },
    Icon {
        name: "Guru",
        file: "guru.png",
        png: include_bytes!("../../../web/public/rank-icons/guru.png"),
    },
    Icon {
        name: "Legend",
        file: "legend.png",
        png: include_bytes!("../../../web/public/rank-icons/legend.png"),
    },
    Icon {
        name: "Guardian",
        file: "guardian.png",
        png: include_bytes!("../../../web/public/rank-icons/guardian.png"),
    },
    Icon {
        name: "TITAN",
        file: "titan.png",
        png: include_bytes!("../../../web/public/rank-icons/titan.png"),
    },
    Icon {
        name: "SAGE",
        file: "sage.png",
        png: include_bytes!("../../../web/public/rank-icons/sage.png"),
    },
    Icon {
        name: "VANGUARD",
        file: "vanguard.png",
        png: include_bytes!("../../../web/public/rank-icons/vanguard.png"),
    },
    Icon {
        name: "ASCENDED",
        file: "ascended.png",
        png: include_bytes!("../../../web/public/rank-icons/ascended.png"),
    },
    Icon {
        name: "DEITY",
        file: "deity.png",
        png: include_bytes!("../../../web/public/rank-icons/deity.png"),
    },
    Icon {
        name: "MYTHIC",
        file: "mythic.png",
        png: include_bytes!("../../../web/public/rank-icons/mythic.png"),
    },
    Icon {
        name: "IMMORTAL",
        file: "immortal.png",
        png: include_bytes!("../../../web/public/rank-icons/immortal.png"),
    },
];
