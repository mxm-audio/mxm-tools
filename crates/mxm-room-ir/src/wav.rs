//! 32-bit IEEE float WAV, written by hand, and a reader for the PCM and float WAVs measured
//! benchmarks ship.
//!
//! RIFF/WAVE with a 18-byte `fmt ` chunk (format tag 3, `cbSize` 0), a `fact` chunk carrying the
//! frame count as the WAVE specification asks of non-PCM data, and interleaved little-endian
//! samples. No dependency: `mxm-fx-convolution` decodes it with `hound`, and the test below checks
//! the header field by field.

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

/// Decodes a PCM (8, 16, 24 or 32 bit) or IEEE float (32 or 64 bit) WAV, plain or extensible.
/// PCM is scaled to ±1; float is kept as written.
pub fn decode(bytes: &[u8]) -> Result<Decoded, Error> {
    let bad = |why: &str| Error::InvalidOption(format!("WAV: {why}"));
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
    let u32_at =
        |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    let mut format = None;
    let mut data = None;
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let len = u32_at(i + 4) as usize;
        let body = i + 8;
        let end = body
            .checked_add(len)
            .filter(|&e| e <= bytes.len())
            .ok_or_else(|| bad("a chunk runs past the end"))?;
        match &bytes[i..i + 4] {
            b"fmt " if len >= 16 => {
                let mut tag = u16_at(body);
                if tag == 0xFFFE && len >= 26 {
                    tag = u16_at(body + 24);
                }
                format = Some((tag, u16_at(body + 2), u32_at(body + 4), u16_at(body + 14)));
            }
            b"data" => data = Some(&bytes[body..end]),
            _ => {}
        }
        i = end + (len & 1);
    }
    let (tag, channels, sample_rate, bits) = format.ok_or_else(|| bad("no fmt chunk"))?;
    let data = data.ok_or_else(|| bad("no data chunk"))?;
    let (count, width) = (usize::from(channels), usize::from(bits / 8));
    if count == 0 || sample_rate == 0 || width == 0 || bits % 8 != 0 {
        return Err(bad("the format has no channels, rate or sample width"));
    }
    let sample: fn(&[u8]) -> f32 = match (tag, bits) {
        (1, 8) => |b| (f32::from(b[0]) - 128.0) / 128.0,
        (1, 16) => |b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
        (1, 24) => |b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
        (1, 32) => {
            |b| (f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_648.0) as f32
        }
        (3, 32) => |b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        (3, 64) => |b| f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]) as f32,
        _ => {
            return Err(bad(&format!(
                "format {tag} at {bits} bits is not supported"
            )));
        }
    };
    let frame = count * width;
    let frames = data.len() / frame;
    let mut out = vec![Vec::with_capacity(frames); count];
    for f in 0..frames {
        for (c, channel) in out.iter_mut().enumerate() {
            let at = f * frame + c * width;
            channel.push(sample(&data[at..at + width]));
        }
    }
    Ok(Decoded {
        channels: out,
        sample_rate,
    })
}

/// Reads and decodes a WAV file.
pub fn read(path: &Path) -> io::Result<Decoded> {
    let bytes = std::fs::read(path)?;
    decode(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_reads_back_float_and_pcm() {
        let left = [0.5f32, -0.25, 0.0];
        let right = [1.0f32, 0.125, -1.0];
        let decoded = decode(&encode_f32(&[&left, &right], 44_100).unwrap()).unwrap();
        assert_eq!(decoded.sample_rate, 44_100);
        assert_eq!(decoded.channels, vec![left.to_vec(), right.to_vec()]);
        // A mono 16-bit PCM file with an odd-length chunk before its data.
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
        b.extend_from_slice(b"LIST");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 0]);
        b.extend_from_slice(b"data");
        b.extend_from_slice(&4u32.to_le_bytes());
        for v in [16_384i16, -32_768] {
            b.extend_from_slice(&v.to_le_bytes());
        }
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
