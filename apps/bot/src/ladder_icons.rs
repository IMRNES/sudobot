//! The badge artwork for each ladder rung, embedded in the binary.
//!
//! Discord takes a role icon as base64 image data in the role edit, so the bytes
//! have to be *somewhere* the bot can read at boot. Two alternatives were
//! rejected:
//!
//! * **Fetching them from the dashboard's public URL** makes a cosmetic role
//!   property depend on an HTTP origin being up. A deploy could leave every rung
//!   without a badge because the web host happened to be mid-restart, and the
//!   failure would look like a Discord permission error.
//! * **Reading them from disk at runtime** needs a path that exists on the host.
//!   The release layout is `bin/` plus `web/`, so it would work — until someone
//!   runs the bot another way and gets a silent "no badges" boot.
//!
//! `include_bytes!` is the version that cannot fail at runtime: a missing or
//! renamed asset stops the *build*, and the deploy is the one place that should
//! notice. The cost is about a megabyte in the executable.
//!
//! The artwork itself lives in `web/public/rank-icons/`, which the dashboard also
//! serves — so a member's profile picture of a rung and the rung's own badge are
//! the same file.

/// Every rung's badge: the ladder name, the file name, and the bytes.
///
/// The file name is carried rather than derived so the test can hold it against
/// [`iam::domain::leveling::LadderRung::icon_path`], which is what the dashboard
/// requests. Recomputing the slug here would be a second copy of the rule and
/// would agree with itself no matter which side changed.
struct Icon {
    /// The rung name, exactly as the ladder spells it.
    name: &'static str,
    /// The file under `web/public/rank-icons/`.
    file: &'static str,
    /// The PNG, embedded at compile time.
    png: &'static [u8],
}

include!("ladder_icons_table.rs");

/// A rung's badge, ready to hand to Discord.
pub struct Badge {
    /// The PNG bytes.
    pub png: &'static [u8],
    /// The file name, used as the upload's filename. Discord echoes it in the
    /// role's audit-log entry, so a badge that arrived from the wrong rung is
    /// identifiable there instead of being an anonymous blob.
    pub file: &'static str,
}

/// The badge for a rung, or `None` for a name that is not in the ladder.
///
/// `None` rather than a panic for an unknown name: the database can hold a rung
/// an administrator renamed by hand, and a missing badge must not take down a
/// reconciliation sweep.
///
/// Returns `None` for bytes that are not a PNG as well. Discord answers a wrong
/// format with a message about the *image*, which reads as a Discord problem when
/// the real cause is a file in this repo; refusing here names the rung instead.
pub fn badge(name: &str) -> Option<Badge> {
    icon_for(name)
        .filter(|icon| is_png(icon.png))
        .map(|icon| Badge {
            png: icon.png,
            file: icon.file,
        })
}

/// The table entry for a rung name, for the tests that check the table's own
/// claims (its file names against the dashboard's `icon_path`).
fn icon_for(name: &str) -> Option<&'static Icon> {
    ICONS.iter().find(|icon| icon.name == name)
}

/// Whether the bytes look like a PNG, which is what Discord decodes.
///
/// The generator writes PNG, but a JPEG dropped in by hand would be sent with a
/// `data:image/png` prefix by serenity and rejected by Discord with a message
/// about the image rather than about the file. Checking the signature here turns
/// that into a legible failure.
pub fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_rung_has_a_badge() {
        assert_eq!(
            ICONS.len(),
            iam::domain::leveling::LADDER.len(),
            "one badge per rung"
        );

        for rung in iam::domain::leveling::LADDER.iter() {
            let bytes = badge(rung.name)
                .map(|b| b.png)
                .unwrap_or_else(|| panic!("{} has no embedded badge", rung.name));
            assert!(
                is_png(bytes),
                "{} is not a PNG ({} bytes, starts {:02X?})",
                rung.name,
                bytes.len(),
                &bytes[..bytes.len().min(8)]
            );
            // A truncated or placeholder asset still passes the signature check,
            // so assert the artwork is actually there.
            assert!(
                bytes.len() > 2_000,
                "{} looks empty at {} bytes",
                rung.name,
                bytes.len()
            );
        }
    }

    #[test]
    fn the_embedded_file_is_the_one_the_dashboard_requests() {
        // The dashboard asks for `icon_path()`, the bot uploads the embedded
        // bytes. If those two ever name different files, a member's badge and the
        // dashboard's picture of it would disagree.
        for rung in iam::domain::leveling::LADDER.iter() {
            let icon = icon_for(rung.name)
                .unwrap_or_else(|| panic!("{} has no embedded badge", rung.name));
            assert_eq!(
                rung.icon_path(),
                format!("rank-icons/{}", icon.file),
                "{}",
                rung.name
            );
        }
    }

    #[test]
    fn every_badge_is_a_file_that_exists_on_disk() {
        // The table could name a file the artwork does not have; `include_bytes!`
        // would catch that at build time, but only for the files it actually
        // embeds. This walks what the table claims and checks it is real, so a
        // stale entry after a rename is caught here rather than as a 404 in the
        // dashboard.
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/public/rank-icons");
        for icon in ICONS.iter() {
            let path = dir.join(icon.file);
            assert!(
                path.is_file(),
                "{} is missing: {}",
                icon.name,
                path.display()
            );
        }
    }

    #[test]
    fn a_name_outside_the_ladder_has_no_badge() {
        assert!(badge("Admin").is_none());
        assert!(badge("neophyte").is_none(), "matching is exact");
    }

    #[test]
    fn a_jpeg_is_not_mistaken_for_a_png() {
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        assert!(!is_png(&jpeg));
        assert!(is_png(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]));
    }
}
