//! 32-bit IEEE float WAV, written by hand; reading through the collection's decoder.
//!
//! RIFF/WAVE with a 18-byte `fmt ` chunk (format tag 3, `cbSize` 0), a `fact` chunk carrying the
//! frame count as the WAVE specification asks of non-PCM data, and interleaved little-endian
//! samples. The writer has no dependency: `mxm-fx-convolution` decodes it with `hound`, and the test
//! below checks//! the header field by field.

use std::io::Cursor;

use mxm_audio_file_decode::{AtLimit, DEFAULT_MAX_SOURCE_CHANNELS, Keep, Limits};

use std::io;
use std::path::Path;

use crate::error::Error;

/// Encodes equal-length channels as a float WAV.
pub fn encode_f32(channels: &[&[f32]], sample_rate: u32) -> Result<Vec<u8>, Error> {
    let count = channels.len();
    if count == 0 || count > u16::MAX as usize || sample_rate == 0 {
        return Err(Error::InvalidOption(
            "a WAV needs at least one channel and a sample rate".into(),
        ));
    }
    let frames = channels[0].len();
    if channels.iter().any(|c| c.len() != frames) {
        return Err(Error::InvalidOption(
            "WAV channels must have equal length".into(),
        ));
    }
    let data_len = frames
        .checked_mul(count * 4)
        .filter(|&n| n <= (u32::MAX as usize) - 64)
        .ok_or_else(|| Error::InvalidOption("WAV larger than 4 GB".into()))?;
    let mut out = Vec::with_capacity(58 + data_len);
    let u16le = |v: u16| v.to_le_bytes();
    let u32le = |v: u32| v.to_le_bytes();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&u32le((4 + 26 + 12 + 8 + data_len) as u32));
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&u32le(18));
    out.extend_from_slice(&u16le(3));
    out.extend_from_slice(&u16le(count as u16));
    out.extend_from_slice(&u32le(sample_rate));
    out.extend_from_slice(&u32le(sample_rate * count as u32 * 4));
    out.extend_from_slice(&u16le(count as u16 * 4));
    out.extend_from_slice(&u16le(32));
    out.extend_from_slice(&u16le(0));
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&u32le(4));
    out.extend_from_slice(&u32le(frames as u32));
    out.extend_from_slice(b"data");
    out.extend_from_slice(&u32le(data_len as u32));
    for frame in 0..frames {
        for channel in channels {
            out.extend_from_slice(&channel[frame].to_le_bytes());
        }
    }
    Ok(out)
}

/// Writes a float WAV, creating parent directories.
pub fn write_f32(path: &Path, channels: &[&[f32]], sample_rate: u32) -> io::Result<()> {
    let bytes = encode_f32(channels, sample_rate)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

/// A decoded WAV: one sample vector per channel.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub channels: Vec<Vec<f32>>,
    pub sample_rate: u32,
}

/// The decoder's bounds for a file read here: nothing truncated and every channel kept, so
/// `catalogue::consumer_check` still sees a long or many-channel file.
fn limits() -> Limits {
    Limits::new(
        usize::MAX,
        AtLimit::Refuse,
        Keep::AllUpTo(DEFAULT_MAX_SOURCE_CHANNELS),
    )
}

fn deinterleave(decoded: mxm_audio_file_decode::Decoded) -> Decoded {
    let count = decoded.channels.max(1);
    let channels = (0..count)
        .map(|c| {
            decoded
                .interleaved
                .iter()
                .skip(c)
                .step_by(count)
                .copied()
                .collect()
        })
        .collect();
    Decoded {
        channels,
        sample_rate: decoded.sample_rate,
    }
}

/// Decodes a PCM (8, 16, 24 or 32 bit) or IEEE float (32 or 64 bit) WAV, plain or extensible, through
/// `mxm-audio-file-decode`. PCM is scaled to ±1; float is kept as written. A non-finite sample is
/// refused here, before `catalogue::consumer_check` would refuse it.
pub fn decode(bytes: &[u8]) -> Result<Decoded, Error> {
    mxm_audio_file_decode::decode(
        Box::new(Cursor::new(bytes.to_vec())),
        Some("wav"),
        &limits(),
    )
    .map(deinterleave)
    .map_err(|e| Error::InvalidOption(format!("WAV: {e}")))
}

/// Reads and decodes a WAV file, as [`decode`].
pub fn read(path: &Path) -> io::Result<Decoded> {
    mxm_audio_file_decode::decode_file(path, &limits())
        .map(deinterleave)
        .map_err(|e| match e {
            mxm_audio_file_decode::Error::Io(e) => e,
            e => io::Error::new(io::ErrorKind::InvalidData, e.to_string()),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A WAV of `tag` (1 PCM, 3 float) at `bits`, plain or extensible, holding `data`.
    fn wav(tag: u16, channels: u16, bits: u16, extensible: bool, data: &[u8]) -> Vec<u8> {
        let rate = 48_000u32;
        let align = channels * bits / 8;
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&(if extensible { 0xFFFE } else { tag }).to_le_bytes());
        fmt.extend_from_slice(&channels.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
        fmt.extend_from_slice(&align.to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        if extensible {
            fmt.extend_from_slice(&22u16.to_le_bytes());
            fmt.extend_from_slice(&bits.to_le_bytes());
            fmt.extend_from_slice(&((1u32 << channels) - 1).to_le_bytes());
            fmt.extend_from_slice(&tag.to_le_bytes());
            fmt.extend_from_slice(&[0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
        }
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&((4 + 8 + fmt.len() + 8 + data.len()) as u32).to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        b.extend_from_slice(&fmt);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    /// Every format the crate reads, with each width's extremes: PCM scaled to ±1 by its full scale,
    /// float kept as written, plain or extensible. These are the scalings the private reader used,
    /// and the shared decoder gave its samples bit for bit on every format (O6's proof, 2026-10-15).
    #[test]
    fn reads_every_format_at_its_scaling() {
        let pcm = |bits: u16, codes: &[i64]| -> Vec<u8> {
            codes
                .iter()
                .flat_map(|&c| c.to_le_bytes()[..usize::from(bits / 8)].to_vec())
                .collect()
        };
        let cases: [(u16, u16, Vec<u8>, Vec<f32>); 6] = [
            (
                1,
                8,
                vec![0, 128, 255, 64],
                vec![-1.0, 0.0, 127.0 / 128.0, -0.5],
            ),
            (
                1,
                16,
                pcm(16, &[-32_768, 0, 32_767, 16_384]),
                vec![-1.0, 0.0, 32_767.0 / 32_768.0, 0.5],
            ),
            (
                1,
                24,
                pcm(24, &[-8_388_608, 0, 8_388_607, 4_194_304]),
                vec![-1.0, 0.0, 8_388_607.0 / 8_388_608.0, 0.5],
            ),
            (
                1,
                32,
                pcm(32, &[-2_147_483_648, 0, 2_147_483_647, 1 << 30]),
                vec![-1.0, 0.0, 1.0, 0.5],
            ),
            (
                3,
                32,
                [-1.0f32, 0.0, 0.25, 1.5]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect(),
                vec![-1.0, 0.0, 0.25, 1.5],
            ),
            (
                3,
                64,
                [-1.0f64, 0.0, 0.1, 1.5]
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect(),
                vec![-1.0, 0.0, 0.1f64 as f32, 1.5],
            ),
        ];
        for (tag, bits, data, expected) in cases {
            for extensible in [false, true] {
                let decoded = decode(&wav(tag, 1, bits, extensible, &data))
                    .unwrap_or_else(|e| panic!("tag {tag} at {bits} bits: {e}"));
                assert_eq!(decoded.sample_rate, 48_000);
                assert_eq!(
                    decoded.channels,
                    vec![expected.clone()],
                    "tag {tag} at {bits} bits, extensible {extensible}"
                );
            }
        }
    }

    #[test]
    fn decode_reads_back_float_and_pcm() {
        let left = [0.5f32, -0.25, 0.0];
        let right = [1.0f32, 0.125, -1.0];
        let decoded = decode(&encode_f32(&[&left, &right], 44_100).unwrap()).unwrap();
        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(decoded.channels, vec![left.to_vec(), right.to_vec()]);
        // A mono 16-bit PCM file with an odd-length chunk, and its pad byte, before its data. (The
        // shared decoder refuses a zero RIFF size and a malformed LIST chunk, which the private
        // reader read past.)
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(b"WAVEfmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        for v in [1u16, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&8_000u32.to_le_bytes());
        b.extend_from_slice(&16_000u32.to_le_bytes());
        for v in [2u16, 16] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(b"JUNK");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 0]);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&4u32.to_le_bytes());
        for v in [16_384i16, -32_768] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let riff = (b.len() - 8) as u32;
        b[4..8].copy_from_slice(&riff.to_le_bytes());
        let pcm = decode(&b).unwrap();
        assert_eq!(pcm.channels, vec![vec![0.5f32, -1.0]]);
        assert!(decode(b"RIFF\0\0\0\0WAVE").is_err());
    }

    fn u16_at(b: &[u8], i: usize) -> u16 {
        u16::from_le_bytes([b[i], b[i + 1]])
    }
    fn u32_at(b: &[u8], i: usize) -> u32 {
        u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
    }

    #[test]
    fn header_and_samples_round_trip() {
        let left = [0.5f32, -0.25, 0.0];
        let right = [1.0f32, 0.125, -1.0];
        let b = encode_f32(&[&left, &right], 48_000).unwrap();
        assert_eq!(&b[0..4], b"RIFF");
        assert_eq!(u32_at(&b, 4) as usize, b.len() - 8);
        assert_eq!(&b[8..16], b"WAVEfmt ");
        assert_eq!(u32_at(&b, 16), 18);
        assert_eq!(u16_at(&b, 20), 3, "IEEE float");
        assert_eq!(u16_at(&b, 22), 2);
        assert_eq!(u32_at(&b, 24), 48_000);
        assert_eq!(u32_at(&b, 28), 48_000 * 8);
        assert_eq!(u16_at(&b, 32), 8);
        assert_eq!(u16_at(&b, 34), 32);
        assert_eq!(&b[38..42], b"fact");
        assert_eq!(u32_at(&b, 46), 3);
        assert_eq!(&b[50..54], b"data");
        assert_eq!(u32_at(&b, 54), 24);
        let sample = |i: usize| {
            f32::from_le_bytes([b[58 + i * 4], b[59 + i * 4], b[60 + i * 4], b[61 + i * 4]])
        };
        assert_eq!(
            [
                sample(0),
                sample(1),
                sample(2),
                sample(3),
                sample(4),
                sample(5)
            ],
            [0.5, 1.0, -0.25, 0.125, 0.0, -1.0]
        );
    }

    #[test]
    fn rejects_ragged_channels() {
        assert!(encode_f32(&[&[0.0, 1.0], &[0.0]], 48_000).is_err());
        assert!(encode_f32(&[], 48_000).is_err());
    }
}
