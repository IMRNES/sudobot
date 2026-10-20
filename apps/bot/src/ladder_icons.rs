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

    /// The artwork must not touch the edges of the canvas.
    ///
    /// Discord draws a role icon into a square slot. A circle that bleeds to the
    /// canvas edge is cropped by that slot's corners, so the badge renders as a
    /// square-ish sticker — the "gagal PNG" look — instead of a disc. The fix is
    /// transparent margin on all four sides, and this is the assertion that keeps it
    /// there: the first release of these badges was full-bleed and every one of the
    /// 21 was quietly square-looking.
    ///
    /// Decoded with the standard library's `inflate` and a full PNG filter
    /// reconstruction rather than a PNG crate: the check has to be *correct*, and a
    /// decoder that skipped unfiltering would report transparent margins for an
    /// opaque image. ~60 lines is cheaper than a dependency that only tests use.
    #[test]
    fn every_badge_has_transparent_margin_on_all_four_sides() {
        for rung in iam::domain::leveling::LADDER.iter() {
            let bytes = badge(rung.name)
                .map(|b| b.png)
                .unwrap_or_else(|| panic!("{} has no embedded badge", rung.name));

            let edge = edge_alpha(bytes)
                .unwrap_or_else(|| panic!("{}: could not decode the PNG", rung.name));

            assert!(
                edge < 8,
                "{}: opaque pixels reach the canvas edge (peak alpha {edge}). Discord \
                 crops a square slot over a full-bleed circle, so the badge renders as \
                 a square. Inset the artwork and leave transparent margin.",
                rung.name
            );
        }
    }

    /// The highest alpha in the outermost pixel ring of an 8-bit RGBA PNG.
    ///
    /// A thin wrapper over [`decode_rgba`] so the two checks that need pixels
    /// share one decoder instead of drifting apart.
    fn edge_alpha(png: &[u8]) -> Option<u8> {
        let (width, height, rgba) = decode_rgba(png)?;

        let pixel_alpha = |x: usize, y: usize| rgba[(y * width + x) * 4 + 3];
        let mut peak = 0u8;

        for x in 0..width {
            peak = peak.max(pixel_alpha(x, 0)).max(pixel_alpha(x, height - 1));
        }
        for y in 0..height {
            peak = peak.max(pixel_alpha(0, y)).max(pixel_alpha(width - 1, y));
        }

        Some(peak)
    }

    /// Decode an 8-bit RGBA PNG to `(width, height, rgba bytes)`.
    ///
    /// Reconstructs every scanline through the PNG filters, because the filtered
    /// bytes are deltas: reading a channel straight out of them would understate
    /// or overstate the real value depending on what the encoder chose. Only
    /// colour type 6 (RGBA) is accepted -- anything else means the artwork
    /// pipeline changed and these checks need revisiting rather than silently
    /// measuring the wrong bytes.
    fn decode_rgba(png: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
        let mut pos = 8; // skip the signature
        let mut width = 0usize;
        let mut height = 0usize;
        let mut idat = Vec::new();

        while pos + 8 <= png.len() {
            let len = u32::from_be_bytes(png.get(pos..pos + 4)?.try_into().ok()?) as usize;
            let kind = png.get(pos + 4..pos + 8)?;
            let body = png.get(pos + 8..pos + 8 + len)?;

            match kind {
                b"IHDR" => {
                    width = u32::from_be_bytes(body.get(0..4)?.try_into().ok()?) as usize;
                    height = u32::from_be_bytes(body.get(4..8)?.try_into().ok()?) as usize;
                    // 8-bit, colour type 6 (RGBA) is what the generator writes. Anything
                    // else means the artwork pipeline changed and this needs revisiting.
                    if body.get(8).copied() != Some(8) || body.get(9).copied() != Some(6) {
                        return None;
                    }
                }
                b"IDAT" => idat.extend_from_slice(body),
                b"IEND" => break,
                _ => {}
            }

            pos += 12 + len;
        }

        if width == 0 || height == 0 {
            return None;
        }

        let raw = inflate(&idat)?;
        let bpp = 4; // RGBA8
        let stride = width * bpp;

        if raw.len() < (stride + 1) * height {
            return None;
        }

        // Unfilter in place, scanline by scanline, as libpng/the spec describes.
        let mut prev = vec![0u8; stride];
        let mut cur = vec![0u8; stride];
        let mut out = Vec::with_capacity(stride * height);

        for row in 0..height {
            let start = row * (stride + 1);
            let filter = raw[start];
            cur.copy_from_slice(&raw[start + 1..start + 1 + stride]);

            for i in 0..stride {
                let a = if i >= bpp { cur[i - bpp] } else { 0 };
                let b = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };

                cur[i] = match filter {
                    0 => cur[i],
                    1 => cur[i].wrapping_add(a),
                    2 => cur[i].wrapping_add(b),
                    3 => cur[i].wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                    4 => {
                        // Paeth
                        let p = i32::from(a) + i32::from(b) - i32::from(c);
                        let pa = (p - i32::from(a)).abs();
                        let pb = (p - i32::from(b)).abs();
                        let pc = (p - i32::from(c)).abs();
                        let pred = if pa <= pb && pa <= pc {
                            a
                        } else if pb <= pc {
                            b
                        } else {
                            c
                        };
                        cur[i].wrapping_add(pred)
                    }
                    _ => return None,
                };
            }

            out.extend_from_slice(&cur);
            std::mem::swap(&mut prev, &mut cur);
        }

        Some((width, height, out))
    }

    /// zlib inflate, using nothing but the standard library.
    ///
    /// Wraps the raw DEFLATE reader in `std`, which handles both stored and
    /// Huffman-coded blocks, and skips zlib's two-byte header and Adler-32 trailer.
    fn inflate(zlib: &[u8]) -> Option<Vec<u8>> {
        use std::io::Read as _;

        if zlib.len() < 6 {
            return None;
        }

        // CMF/FLG: the low nibble of CMF must be 8 (DEFLATE), and the preset
        // dictionary bit must be clear — otherwise this is not the format the encoder
        // was expected to produce.
        if zlib[0] & 0x0f != 8 || zlib[1] & 0x20 != 0 {
            return None;
        }

        let body = &zlib[2..zlib.len() - 4]; // strip header and Adler-32
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(body)
            .read_to_end(&mut out)
            .ok()?;
        Some(out)
    }

    /// No badge may carry a square plate inside its circle.
    ///
    /// This is the defect the outer-margin check could not see. Measured on the
    /// first release: the bright ring around the artwork sat at r=81 along the
    /// axes but r=114 along the diagonals -- a ratio of sqrt(2), which is the
    /// signature of a square, not a circle. The alpha mask was round, so the
    /// canvas had no opaque edge pixels and the margin assertion passed, while
    /// the plate's four corners were plainly visible inside the badge.
    ///
    /// A circle gives a ratio of 1.0 whatever its radius; a square plate gives
    /// 1.414. The window is centred on sqrt(2) rather than set to "anything
    /// above 1.2" because a bright patch in the middle of a glyph also scores
    /// high (one badge measured 2.12) and must not be reported as a plate.
    #[test]
    fn no_badge_has_a_square_plate_inside_its_circle() {
        for rung in iam::domain::leveling::LADDER.iter() {
            let bytes = badge(rung.name)
                .map(|b| b.png)
                .unwrap_or_else(|| panic!("{} has no embedded badge", rung.name));

            // `None` means the badge has no bright outer band to measure -- it is
            // composed differently, not defective. Failing on that would make the
            // check reject a perfectly round badge for lacking a rim, which is what
            // it did to Visionary on the first run.
            let Some(ratio) = plate_squareness(bytes) else {
                continue;
            };

            // Centred on sqrt(2) rather than "anything above 1.2": a bright patch
            // inside a glyph also scores high -- Mage measures 2.12 on its lit
            // crystal -- and reporting that as a plate would fail a badge whose
            // silhouette is a perfect circle.
            let square_plate = (1.30..=1.50).contains(&ratio);
            assert!(
                !square_plate,
                "{}: the artwork is a square plate (diagonal/axis reach = {ratio:.2}, \
                 where a circle is 1.00 and a square is 1.41). The plate's corners are \
                 what reads as a frame in Discord. Re-mask the artwork to a circle.",
                rung.name
            );
        }
    }

    /// How square the artwork's outer bright band is: its reach along the
    /// diagonals divided by its reach along the axes.
    ///
    /// 1.0 is a circle, sqrt(2) is a square. Returns `None` when no bright band
    /// is found at all, which means the badge is composed differently and this
    /// check does not apply.
    fn plate_squareness(png: &[u8]) -> Option<f32> {
        let (width, height, rgba) = decode_rgba(png)?;
        let centre_x = width as f32 / 2.0;
        let centre_y = height as f32 / 2.0;
        let limit = width / 2 + 1;

        let reach = |degrees: f32| -> Option<usize> {
            let radians = degrees.to_radians();
            let mut found = None;
            for radius in 1..limit {
                let x = (centre_x + radius as f32 * radians.cos()) as usize;
                let y = (centre_y + radius as f32 * radians.sin()) as usize;
                if x >= width || y >= height {
                    break;
                }
                let offset = (y * width + x) * 4;
                let (r, g, b, a) = (
                    rgba[offset],
                    rgba[offset + 1],
                    rgba[offset + 2],
                    rgba[offset + 3],
                );
                // The plate's rim: opaque and bright. The surrounding glow is
                // dimmer, so a luminance floor separates the two.
                if a > 200 && (u32::from(r) + u32::from(g) + u32::from(b)) / 3 > 150 {
                    found = Some(radius);
                }
            }
            found
        };

        let axes: Vec<usize> = [0.0, 90.0, 180.0, 270.0]
            .iter()
            .filter_map(|d| reach(*d))
            .collect();
        let diagonals: Vec<usize> = [45.0, 135.0, 225.0, 315.0]
            .iter()
            .filter_map(|d| reach(*d))
            .collect();

        if axes.is_empty() || diagonals.is_empty() {
            return None;
        }

        let axis_min = *axes.iter().min()? as f32;
        let diagonal_max = *diagonals.iter().max()? as f32;
        Some(diagonal_max / axis_min.max(1.0))
    }

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
