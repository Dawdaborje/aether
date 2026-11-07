//! Simulcast: a browser sends its camera at several sizes (layers, named by a `rid`: `h`, `m`, `l`), and each viewer is
//! given one of them. Moving a viewer to another layer is only clean at a key frame, so the server keeps sending the old
//! layer until the new one has a key frame.

use std::time::{Duration, Instant};

use str0m::format::Codec;

use crate::protocol::Quality;

/// How big a layer is, from its name. Browsers are told `h`, `m` and `l`; other names rank below `l`.
pub fn rank(rid: &str) -> u8 {
    match rid {
        "h" | "f" | "high" => 3,
        "m" | "q" | "mid" => 2,
        "l" | "low" => 1,
        _ => 0,
    }
}

/// How long a layer may be silent and still count as being sent.
pub const ALIVE: Duration = Duration::from_millis(1500);

/// One layer of a sender and when a packet of it last arrived.
#[derive(Debug, Clone)]
pub struct Layer {
    pub rid: String,
    pub last: Instant,
}

/// Note a packet of layer `rid`.
pub fn note(layers: &mut Vec<Layer>, rid: &str, now: Instant) {
    match layers.iter_mut().find(|l| l.rid == rid) {
        Some(layer) => layer.last = now,
        None => layers.push(Layer { rid: rid.to_string(), last: now }),
    }
}

/// The names of the layers that are being sent now: one the sender stopped (the browser drops layers when its own
/// connection is short of room) must not be chosen, or its viewers would be left on a frozen picture.
pub fn alive(layers: &[Layer], now: Instant) -> Vec<String> {
    layers.iter().filter(|l| now.saturating_duration_since(l.last) < ALIVE).map(|l| l.rid.clone()).collect()
}

/// The layer a viewer should be given out of those the sender is sending: the biggest for `high`, the smallest for `low`,
/// none for `off`.
pub fn target(seen: &[String], quality: Quality) -> Option<&str> {
    match quality {
        Quality::High => seen.iter().max_by_key(|r| rank(r)),
        Quality::Low => seen.iter().min_by_key(|r| rank(r)),
        Quality::Off => None,
    }
    .map(String::as_str)
}

/// Whether a frame, as the depacketizer hands it over, is a key frame; `None` for a codec this does not know, whose
/// viewers are therefore never moved between layers.
pub fn is_keyframe(codec: Codec, frame: &[u8]) -> Option<bool> {
    match codec {
        // Bit 0 of the VP8 frame tag is 0 for a key frame.
        Codec::Vp8 => Some(frame.first().is_some_and(|b| b & 1 == 0)),
        // VP9 uncompressed header: frame marker (2 bits), profile (2 bits, plus a reserved bit for profile 3),
        // show_existing_frame, frame_type (0 is a key frame).
        Codec::Vp9 => {
            let byte = *frame.first()?;
            let profile = ((byte >> 5) & 1) | (((byte >> 4) & 1) << 1);
            let at = if profile == 3 { 2 } else { 3 };
            let show_existing = (byte >> at) & 1 == 1;
            Some((byte >> 6) == 0b10 && !show_existing && (byte >> (at - 1)) & 1 == 0)
        }
        // Annex B: an IDR slice (NAL type 5) is in the frame.
        Codec::H264 => Some(
            frame.windows(4).any(|w| w[..3] == [0, 0, 1] && w[3] & 0x1f == 5)
                || frame.windows(5).any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 0x1f == 5),
        ),
        _ => None,
    }
}

/// How much video a viewer's connection can take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    /// Anything the viewer asked for.
    Full,
    /// No more than the small layers.
    Small,
}

/// What one big layer and one small layer need (bits per second).
pub const BIG_BPS: u64 = 1_000_000;
pub const SMALL_BPS: u64 = 150_000;

/// The cap for a viewer given the bandwidth estimate of their connection and how many cameras they are sent. It drops as
/// soon as the estimate cannot carry the big pictures and comes back only when there is a good margin, so a number that
/// hovers near the line does not make the picture jump between sizes.
pub fn next_cap(current: Cap, estimate: u64, cameras: usize) -> Cap {
    let need = BIG_BPS * cameras.max(1) as u64;
    match current {
        Cap::Full if estimate < need => Cap::Small,
        Cap::Small if estimate > need + need / 3 => Cap::Full,
        other => other,
    }
}

/// What a viewer is given, from what they asked for and their cap.
pub fn effective(asked: Quality, cap: Cap) -> Quality {
    match (asked, cap) {
        (Quality::High, Cap::Small) => Quality::Low,
        (asked, _) => asked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn high_takes_the_biggest_layer_and_low_the_smallest() {
        let layers = seen(&["l", "h", "m"]);
        assert_eq!(target(&layers, Quality::High), Some("h"));
        assert_eq!(target(&layers, Quality::Low), Some("l"));
        assert_eq!(target(&layers, Quality::Off), None);
    }

    #[test]
    fn a_sender_with_one_layer_gives_it_to_everyone() {
        let layers = seen(&["m"]);
        assert_eq!(target(&layers, Quality::High), Some("m"));
        assert_eq!(target(&layers, Quality::Low), Some("m"));
        assert_eq!(target(&[], Quality::High), None);
    }

    #[test]
    fn vp8_key_frames_are_told_apart_by_the_first_bit() {
        assert_eq!(is_keyframe(Codec::Vp8, &[0x10, 0x02, 0x00]), Some(true));
        assert_eq!(is_keyframe(Codec::Vp8, &[0x11, 0x02, 0x00]), Some(false));
        assert_eq!(is_keyframe(Codec::Vp8, &[]), Some(false));
    }

    #[test]
    fn h264_key_frames_hold_an_idr_slice() {
        assert_eq!(is_keyframe(Codec::H264, &[0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x65, 9]), Some(true));
        assert_eq!(is_keyframe(Codec::H264, &[0, 0, 0, 1, 0x65, 9]), Some(true));
        assert_eq!(is_keyframe(Codec::H264, &[0, 0, 0, 1, 0x41, 9, 9]), Some(false));
    }

    #[test]
    fn vp9_key_frames_have_frame_type_zero() {
        // frame marker 10, profile 0, show_existing 0, frame_type 0 => 1000_0000; frame_type 1 => 1000_0100
        assert_eq!(is_keyframe(Codec::Vp9, &[0b1000_0000]), Some(true));
        assert_eq!(is_keyframe(Codec::Vp9, &[0b1000_0100]), Some(false));
        assert_eq!(is_keyframe(Codec::Vp9, &[0b1000_1000]), Some(false)); // show_existing_frame
        assert_eq!(is_keyframe(Codec::Vp9, &[0b0000_0000]), Some(false)); // not a VP9 frame marker
    }

    #[test]
    fn a_codec_it_does_not_know_is_never_switched() {
        assert_eq!(is_keyframe(Codec::Opus, &[0, 0, 0]), None);
    }

    #[test]
    fn a_layer_that_went_quiet_is_not_chosen() {
        let start = Instant::now();
        let mut layers = Vec::new();
        note(&mut layers, "h", start);
        note(&mut layers, "l", start);
        let later = start + Duration::from_millis(1000);
        note(&mut layers, "l", later); // only the small one goes on
        assert_eq!(alive(&layers, later).len(), 2);
        let much_later = start + Duration::from_millis(2000);
        note(&mut layers, "l", much_later);
        assert_eq!(alive(&layers, much_later), vec!["l".to_string()]);
        assert_eq!(target(&alive(&layers, much_later), Quality::High), Some("l"));
        note(&mut layers, "h", much_later); // it came back
        assert_eq!(target(&alive(&layers, much_later), Quality::High), Some("h"));
    }

    #[test]
    fn the_cap_drops_fast_and_returns_with_a_margin() {
        assert_eq!(next_cap(Cap::Full, 3_000_000, 2), Cap::Full);
        assert_eq!(next_cap(Cap::Full, 1_900_000, 2), Cap::Small); // two big cameras need 2 Mbit/s
        assert_eq!(next_cap(Cap::Small, 2_100_000, 2), Cap::Small); // just above the line is not enough
        assert_eq!(next_cap(Cap::Small, 2_700_000, 2), Cap::Full);
        assert_eq!(next_cap(Cap::Full, 900_000, 0), Cap::Small); // no cameras still counts as one
    }

    #[test]
    fn a_capped_viewer_is_never_given_more_than_small() {
        assert_eq!(effective(Quality::High, Cap::Small), Quality::Low);
        assert_eq!(effective(Quality::High, Cap::Full), Quality::High);
        assert_eq!(effective(Quality::Off, Cap::Small), Quality::Off);
        assert_eq!(effective(Quality::Low, Cap::Full), Quality::Low);
    }
}
