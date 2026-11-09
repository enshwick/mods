//! The mod's test sound: one second of a 440 Hz tone, 16-bit mono at the
//! game's sample rate, written into the mod's folder at start so no binary
//! file ships in the source.

use enshwick::sound::SAMPLE_RATE;

/// The tone's pitch, in hertz.
const PITCH: f64 = 440.0;
/// Its loudness, as a share of full scale.
const LOUDNESS: f64 = 0.5;

/// The tone as a WAV file.
#[expect(
    clippy::cast_possible_truncation,
    reason = "each sample stays within half of the i16 range"
)]
pub(crate) fn wav() -> Vec<u8> {
    let frames = SAMPLE_RATE;
    let data = frames * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    // PCM, one channel.
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    // Two bytes a frame, 16 bits a sample.
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    let step = core::f64::consts::TAU * PITCH / f64::from(SAMPLE_RATE);
    for frame in 0..frames {
        let sample = (f64::from(frame) * step).sin() * LOUDNESS * f64::from(i16::MAX);
        out.extend_from_slice(&(sample as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tone_is_one_second_of_16_bit_mono_at_the_game_s_rate() {
        let wav = wav();
        let word = |at: usize| u32::from_le_bytes(wav[at..at + 4].try_into().unwrap());
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(word(24), SAMPLE_RATE);
        assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 1, "mono");
        assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16, "16-bit");
        assert_eq!(word(40) as usize, wav.len() - 44);
        assert_eq!(wav.len() - 44, SAMPLE_RATE as usize * 2, "one second");
        assert!(wav.len() as u64 <= enshwick::sound::MAX_FILE_BYTES);
    }

    #[test]
    fn the_tone_crosses_zero_about_880_times_a_second() {
        let wav = wav();
        let samples: Vec<i16> = wav[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| i16::from_le_bytes(*pair))
            .collect();
        let crossings = samples
            .windows(2)
            .filter(|w| (w[0] < 0) != (w[1] < 0))
            .count();
        assert!((878..=882).contains(&crossings), "{crossings}");
    }
}
