//! WEM -> a standard audio file the webview (and any player) opens (addendum 7.16):
//!
//! | fmt tag            | codec            | output                                   |
//! |--------------------|------------------|------------------------------------------|
//! | `0xFFFF`           | Wwise Vorbis     | `.ogg` (ww2ogg, aoTuV codebooks)          |
//! | `0x0001`, `0xFFFE` | PCM              | `.wav` (the Wwise chunks dropped)        |
//! | `0x0002`, `0x0069` | Wwise IMA ADPCM  | `.wav`, decoded to 16-bit PCM            |
//! | `0x3040`           | Opus (Ogg)       | `.opus` (the Ogg stream inside the WEM)  |
//! | `0x3041`           | Wwise Opus       | `.opus` (its packets put in an Ogg Opus) |
//! | anything else      | named, kept      | the `.wem` itself                        |
//!
//! `PLUG` media (Wwise plugin data such as convolution-reverb impulse responses) is not audio
//! and is named as such. Layouts follow vgmstream's `wwise.c` / `ima_decoder.c`.
//!
//! The Vorbis path first runs StarBreaker's `vorbis_to_ogg`. Its quality check rejects loud,
//! clipped sounds (more than 10% of one decoded packet at full scale: impacts, gunshots,
//! explosions), so on its error the WEM is converted again with the aoTuV codebooks and kept
//! when lewton decodes the whole stream without error.

use std::fmt;
use std::io::Cursor;

/// A Vorbis stream whose decoded samples sit at full scale this often is taken as garbage.
pub const MAX_CLIP_RATIO: f64 = 0.25;

/// What a WEM holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Codec {
    Vorbis,
    Pcm,
    ImaAdpcm,
    /// `0x3040`: an Ogg Opus stream in the data chunk.
    Opus,
    /// `0x3041`: Opus packets, their sizes in the `seek` chunk.
    WwiseOpus,
    /// `PLUG`: Wwise plugin data, not audio.
    Plugin,
    /// Another fmt tag.
    Other(u16),
}

impl Codec {
    pub fn from_tag(tag: u16) -> Self {
        match tag {
            0xFFFF => Self::Vorbis,
            0x0001 | 0xFFFE => Self::Pcm,
            0x0002 | 0x0069 => Self::ImaAdpcm,
            0x3040 => Self::Opus,
            0x3041 => Self::WwiseOpus,
            t => Self::Other(t),
        }
    }

    /// The file this codec becomes, when it is converted.
    pub fn format(self) -> Option<Format> {
        match self {
            Self::Vorbis => Some(Format::Ogg),
            Self::Pcm | Self::ImaAdpcm => Some(Format::Wav),
            Self::Opus | Self::WwiseOpus => Some(Format::Opus),
            Self::Plugin | Self::Other(_) => None,
        }
    }
}

impl fmt::Display for Codec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vorbis => f.write_str("Vorbis"),
            Self::Pcm => f.write_str("PCM"),
            Self::ImaAdpcm => f.write_str("IMA ADPCM"),
            Self::Opus => f.write_str("Opus"),
            Self::WwiseOpus => f.write_str("Wwise Opus"),
            Self::Plugin => f.write_str("Wwise plugin data"),
            Self::Other(0x8311) => f.write_str("PTADPCM (fmt 0x8311)"),
            Self::Other(t) => write!(f, "fmt 0x{t:04x}"),
        }
    }
}

/// A converted sound's file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    Ogg,
    Opus,
    Wav,
}

impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Ogg => "ogg",
            Self::Opus => "opus",
            Self::Wav => "wav",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Ogg | Self::Opus => "audio/ogg",
            Self::Wav => "audio/wav",
        }
    }
}

/// The extensions a sound can be written as, converted first (the asset cache looks them up
/// in this order).
pub const EXTS: [&str; 4] = ["ogg", "opus", "wav", "wem"];

/// The MIME type of a sound file by its extension (empty for `.wem`).
pub fn mime_of(ext: &str) -> &'static str {
    match ext {
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub codec: Codec,
}

/// Why a WEM stays a WEM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unconverted {
    /// `None`: not a WEM at all.
    pub codec: Option<Codec>,
    pub reason: String,
}

impl Unconverted {
    fn new(codec: Codec, reason: impl Into<String>) -> Self {
        Self {
            codec: Some(codec),
            reason: reason.into(),
        }
    }

    /// A sentence for the UI and the CLI.
    pub fn message(&self) -> String {
        match self.codec {
            None => format!("Not a WEM file ({})", self.reason),
            Some(Codec::Plugin) => {
                "Wwise plugin data (such as a reverb impulse response), not audio".into()
            }
            Some(c @ Codec::Other(_)) => {
                format!("{c} audio cannot be converted; it is kept as .wem")
            }
            Some(c) => format!(
                "{c} audio that could not be converted ({}); it is kept as .wem",
                self.reason
            ),
        }
    }
}

impl fmt::Display for Unconverted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

/// The parts of a RIFF WEM the converters read.
struct Riff<'a> {
    fmt: &'a [u8],
    data: &'a [u8],
    seek: Option<&'a [u8]>,
    meta: Option<&'a [u8]>,
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

fn riff(w: &[u8]) -> Result<Riff<'_>, Unconverted> {
    let bad = |why: &str| Unconverted {
        codec: None,
        reason: why.into(),
    };
    if w.len() < 12 || &w[8..12] != b"WAVE" {
        return Err(bad("no RIFF WAVE header"));
    }
    if &w[0..4] == b"RIFX" {
        return Err(bad("big-endian RIFX is not read"));
    }
    if &w[0..4] != b"RIFF" {
        return Err(bad("no RIFF WAVE header"));
    }
    let (mut fmt, mut data, mut seek, mut meta) = (None, None, None, None);
    let mut p = 12usize;
    while p + 8 <= w.len() {
        let id = &w[p..p + 4];
        let size = u32_at(w, p + 4).unwrap_or(0) as usize;
        let start = p + 8;
        let end = start.saturating_add(size).min(w.len());
        let body = &w[start..end];
        match id {
            b"fmt " => fmt = Some(body),
            b"data" => data = Some(body),
            b"seek" => seek = Some(body),
            b"meta" => meta = Some(body),
            _ => {}
        }
        p = start.saturating_add(size).saturating_add(size & 1);
    }
    let fmt = fmt
        .filter(|f| f.len() >= 16)
        .ok_or_else(|| bad("no fmt chunk"))?;
    Ok(Riff {
        fmt,
        data: data.ok_or_else(|| bad("no data chunk"))?,
        seek,
        meta,
    })
}

/// The codec of a WEM (or `PLUG` media), from its header.
pub fn codec_of(w: &[u8]) -> Result<Codec, Unconverted> {
    if w.starts_with(b"PLUG") {
        return Ok(Codec::Plugin);
    }
    let r = riff(w)?;
    Ok(Codec::from_tag(u16_at(r.fmt, 0).unwrap_or(0)))
}

/// Convert a WEM to the standard file its codec becomes.
pub fn convert(w: &[u8]) -> Result<Converted, Unconverted> {
    let codec = codec_of(w)?;
    let Some(format) = codec.format() else {
        return Err(Unconverted::new(codec, "not supported"));
    };
    let r = riff(w)?;
    let bytes = match codec {
        Codec::Vorbis => vorbis(w),
        Codec::Pcm => pcm_wav(&r),
        Codec::ImaAdpcm => ima_wav(&r),
        Codec::Opus => ogg_opus(&r),
        Codec::WwiseOpus => wwise_opus(&r),
        Codec::Plugin | Codec::Other(_) => unreachable!("no format"),
    }
    .map_err(|e| Unconverted::new(codec, e))?;
    Ok(Converted {
        bytes,
        format,
        codec,
    })
}

// ---- Vorbis ---------------------------------------------------------------------------

fn vorbis(w: &[u8]) -> Result<Vec<u8>, String> {
    match starbreaker_wem::decode::vorbis_to_ogg(w) {
        Ok(ogg) => Ok(ogg),
        Err(first) => {
            let cb = ww2ogg::CodebookLibrary::aotuv_codebooks().map_err(|e| e.to_string())?;
            let mut c = ww2ogg::WwiseRiffVorbis::new(Cursor::new(w), cb)
                .map_err(|e| format!("{first}; aoTuV: {e}"))?;
            let mut ogg = Vec::new();
            c.generate_ogg(&mut ogg)
                .map_err(|e| format!("{first}; aoTuV: {e}"))?;
            check_ogg_vorbis(&ogg).map_err(|e| format!("{first}; aoTuV: {e}"))?;
            Ok(ogg)
        }
    }
}

/// Decode a whole Ogg Vorbis stream: no error, some samples, and not mostly at full scale.
pub fn check_ogg_vorbis(ogg: &[u8]) -> Result<u64, String> {
    let mut r = lewton::inside_ogg::OggStreamReader::new(Cursor::new(ogg))
        .map_err(|e| format!("Ogg Vorbis headers: {e}"))?;
    let (mut n, mut clip) = (0u64, 0u64);
    loop {
        match r.read_dec_packet_itl() {
            Ok(Some(p)) => {
                n += p.len() as u64;
                clip += p
                    .iter()
                    .filter(|&&s| s == i16::MAX || s == i16::MIN)
                    .count() as u64;
            }
            Ok(None) => break,
            Err(e) => return Err(format!("Vorbis decode: {e}")),
        }
    }
    if n == 0 {
        return Err("no samples decoded".into());
    }
    if clip as f64 / n as f64 > MAX_CLIP_RATIO {
        return Err(format!("{clip} of {n} samples at full scale"));
    }
    Ok(n)
}

// ---- PCM ------------------------------------------------------------------------------

/// The standard speaker mask for `channels` (WAVE_FORMAT_EXTENSIBLE).
fn default_mask(channels: u16) -> u32 {
    match channels {
        1 => 0x4,
        2 => 0x3,
        3 => 0x7,
        4 => 0x33,
        5 => 0x37,
        6 => 0x3F,
        7 => 0x13F,
        8 => 0x63F,
        _ => 0,
    }
}

/// A RIFF WAVE of little-endian integer PCM.
pub fn wav(channels: u16, rate: u32, bits: u16, mask: u32, pcm: &[u8]) -> Vec<u8> {
    let align = channels * bits.div_ceil(8);
    let ext = channels > 2;
    let fmt_len: u32 = if ext { 40 } else { 16 };
    let data_len = pcm.len() as u32;
    let mut o = Vec::with_capacity(pcm.len() + 68);
    o.extend_from_slice(b"RIFF");
    o.extend_from_slice(&(4 + 8 + fmt_len + 8 + data_len + (data_len & 1)).to_le_bytes());
    o.extend_from_slice(b"WAVEfmt ");
    o.extend_from_slice(&fmt_len.to_le_bytes());
    o.extend_from_slice(&(if ext { 0xFFFEu16 } else { 1 }).to_le_bytes());
    o.extend_from_slice(&channels.to_le_bytes());
    o.extend_from_slice(&rate.to_le_bytes());
    o.extend_from_slice(&(rate * u32::from(align)).to_le_bytes());
    o.extend_from_slice(&align.to_le_bytes());
    o.extend_from_slice(&bits.to_le_bytes());
    if ext {
        o.extend_from_slice(&22u16.to_le_bytes());
        o.extend_from_slice(&bits.to_le_bytes());
        o.extend_from_slice(
            &(if mask == 0 {
                default_mask(channels)
            } else {
                mask
            })
            .to_le_bytes(),
        );
        // KSDATAFORMAT_SUBTYPE_PCM
        o.extend_from_slice(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ]);
    }
    o.extend_from_slice(b"data");
    o.extend_from_slice(&data_len.to_le_bytes());
    o.extend_from_slice(pcm);
    if data_len & 1 == 1 {
        o.push(0);
    }
    o
}

fn pcm_wav(r: &Riff<'_>) -> Result<Vec<u8>, String> {
    let ch = u16_at(r.fmt, 2).unwrap_or(0);
    let rate = u32_at(r.fmt, 4).unwrap_or(0);
    let bits = u16_at(r.fmt, 14).unwrap_or(0);
    if ch == 0 || rate == 0 {
        return Err("no channels or sample rate".into());
    }
    if !matches!(bits, 8 | 16 | 24 | 32) {
        return Err(format!("{bits}-bit PCM"));
    }
    // Wwise's extensible fmt is 0x18 bytes: the channel mask at 0x14, no sub-format GUID.
    let mask = if u16_at(r.fmt, 0) == Some(0xFFFE) {
        u32_at(r.fmt, 20).unwrap_or(0)
    } else {
        0
    };
    let frame = usize::from(ch) * usize::from(bits / 8);
    let n = r.data.len() / frame * frame;
    Ok(wav(ch, rate, bits, mask, &r.data[..n]))
}

// ---- IMA ADPCM ------------------------------------------------------------------------

const IMA_STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

/// Bytes of one channel's block (4-byte header + 32 bytes of nibbles).
pub const IMA_BLOCK: usize = 0x24;
/// Samples one channel's block decodes to (the header sample + 63 nibbles; the last nibble
/// is skipped, as in every XBOX-style IMA).
pub const IMA_SAMPLES: usize = 64;

/// One IMA nibble (the multiply form Wwise uses).
pub fn ima_step(code: u8, hist: &mut i32, index: &mut i32) -> i16 {
    let step = IMA_STEP[*index as usize];
    let mut delta = ((i32::from(code & 7) * 2 + 1) * step) >> 3;
    if code & 8 != 0 {
        delta = -delta;
    }
    *hist = (*hist + delta).clamp(-32768, 32767);
    *index = (*index + IMA_INDEX[usize::from(code & 0xF)]).clamp(0, 88);
    *hist as i16
}

/// Wwise IMA: per block, each channel's 0x24 bytes in turn (header: s16 sample, s8 index).
pub fn ima_decode(data: &[u8], channels: usize) -> Vec<i16> {
    let block = IMA_BLOCK * channels;
    let blocks = data.len() / block;
    let mut out = vec![0i16; blocks * IMA_SAMPLES * channels];
    for b in 0..blocks {
        for c in 0..channels {
            let x = &data[b * block + c * IMA_BLOCK..b * block + (c + 1) * IMA_BLOCK];
            let mut hist = i32::from(i16::from_le_bytes([x[0], x[1]]));
            let mut index = i32::from(x[2] as i8).clamp(0, 88);
            let base = b * IMA_SAMPLES * channels;
            out[base + c] = hist as i16;
            for i in 1..IMA_SAMPLES {
                let byte = x[4 + (i - 1) / 2];
                let code = if (i - 1) & 1 == 1 {
                    byte >> 4
                } else {
                    byte & 0xF
                };
                out[base + i * channels + c] = ima_step(code, &mut hist, &mut index);
            }
        }
    }
    out
}

fn ima_wav(r: &Riff<'_>) -> Result<Vec<u8>, String> {
    let ch = u16_at(r.fmt, 2).unwrap_or(0);
    let rate = u32_at(r.fmt, 4).unwrap_or(0);
    let align = u16_at(r.fmt, 12).unwrap_or(0);
    let bits = u16_at(r.fmt, 14).unwrap_or(0);
    if ch == 0 || rate == 0 {
        return Err("no channels or sample rate".into());
    }
    if bits != 4 || usize::from(align) != IMA_BLOCK * usize::from(ch) {
        return Err(format!("{bits}-bit IMA with {align}-byte blocks"));
    }
    let pcm: Vec<u8> = ima_decode(r.data, usize::from(ch))
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    if pcm.is_empty() {
        return Err("no complete IMA block".into());
    }
    Ok(wav(ch, rate, 16, 0, &pcm))
}

// ---- Opus -----------------------------------------------------------------------------

/// `0x3040`: the data chunk is an Ogg Opus stream (after `meta`'s skip, when there).
fn ogg_opus(r: &Riff<'_>) -> Result<Vec<u8>, String> {
    let skip = r.meta.and_then(|m| u32_at(m, 4)).unwrap_or(0) as usize;
    let d = r.data.get(skip..).unwrap_or_default();
    let at = d
        .windows(4)
        .take(4096)
        .position(|w| w == b"OggS")
        .ok_or("no Ogg stream in the data chunk")?;
    let d = &d[at..];
    if !d.windows(8).take(4096).any(|w| w == b"OpusHead") {
        return Err("no OpusHead".into());
    }
    Ok(d.to_vec())
}

/// Samples (at 48 kHz) of one Opus packet, from its TOC byte (RFC 6716 3.1).
pub fn opus_packet_samples(p: &[u8]) -> Option<u64> {
    let toc = *p.first()?;
    let config = toc >> 3;
    let frame: u64 = match config {
        0..=11 => [480, 960, 1920, 2880][usize::from(config & 3)],
        12..=15 => [480, 960][usize::from(config & 1)],
        _ => [120, 240, 480, 960][usize::from(config & 3)],
    };
    let frames: u64 = match toc & 3 {
        0 => 1,
        1 | 2 => 2,
        _ => u64::from(*p.get(1)? & 0x3F),
    };
    Some(frame * frames)
}

/// Streams and coupled streams of Vorbis-order channel mapping family 1 (RFC 7845 5.1.1.2).
fn family1(channels: u8) -> Option<(u8, u8, &'static [u8])> {
    Some(match channels {
        3 => (2, 1, &[0, 2, 1]),
        4 => (2, 2, &[0, 1, 2, 3]),
        5 => (3, 2, &[0, 4, 1, 2, 3]),
        6 => (4, 2, &[0, 4, 1, 2, 3, 5]),
        7 => (5, 2, &[0, 4, 1, 2, 3, 5, 6]),
        8 => (5, 3, &[0, 6, 1, 2, 3, 4, 5, 7]),
        _ => return None,
    })
}

/// An Ogg Opus file from raw packets (each one an Ogg packet, granules counted at 48 kHz).
pub fn ogg_opus_from_packets(
    channels: u8,
    pre_skip: u16,
    rate: u32,
    mapping: u8,
    total: Option<u64>,
    packets: &[&[u8]],
) -> Result<Vec<u8>, String> {
    use ogg::writing::{PacketWriteEndInfo as End, PacketWriter};
    let mut head = Vec::with_capacity(32);
    head.extend_from_slice(b"OpusHead");
    head.push(1);
    head.push(channels);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&rate.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    if channels <= 2 && mapping != 255 {
        head.push(0);
    } else if mapping == 255 {
        head.extend_from_slice(&[255, channels, 0]);
        head.extend(0..channels);
    } else {
        let (s, c, m) =
            family1(channels).ok_or(format!("{channels} channels, mapping {mapping}"))?;
        head.extend_from_slice(&[1, s, c]);
        head.extend_from_slice(m);
    }
    let mut tags = Vec::new();
    tags.extend_from_slice(b"OpusTags");
    let vendor = b"starorg-codex";
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    let serial = 0x5354_5247;
    let mut out = Vec::new();
    {
        let mut w = PacketWriter::new(&mut out);
        let e = |e: std::io::Error| e.to_string();
        w.write_packet(head.into_boxed_slice(), serial, End::EndPage, 0)
            .map_err(e)?;
        w.write_packet(tags.into_boxed_slice(), serial, End::EndPage, 0)
            .map_err(e)?;
        let end = total.map(|t| t + u64::from(pre_skip));
        let mut gp = 0u64;
        for (i, p) in packets.iter().enumerate() {
            gp += opus_packet_samples(p).ok_or(format!("packet {i} is empty"))?;
            let last = i + 1 == packets.len();
            let g = match (last, end) {
                (true, Some(t)) => t.min(gp),
                _ => gp,
            };
            let inf = if last {
                End::EndStream
            } else if (i + 1) % 50 == 0 {
                End::EndPage
            } else {
                End::NormalPacket
            };
            w.write_packet(p.to_vec().into_boxed_slice(), serial, inf, g)
                .map_err(e)?;
        }
    }
    Ok(out)
}

/// `0x3041`: fmt 0x18 samples, 0x1C frame count, 0x20 pre-skip, 0x22 version (1), 0x23
/// mapping; the `seek` chunk holds each frame's size (u16).
fn wwise_opus(r: &Riff<'_>) -> Result<Vec<u8>, String> {
    let ch = u16_at(r.fmt, 2).unwrap_or(0);
    let samples = u32_at(r.fmt, 0x18).ok_or("fmt chunk too short")?;
    let count = u32_at(r.fmt, 0x1C).ok_or("fmt chunk too short")? as usize;
    let skip = u16_at(r.fmt, 0x20).ok_or("fmt chunk too short")?;
    let version = r.fmt.get(0x22).copied().unwrap_or(0);
    let mapping = r.fmt.get(0x23).copied().unwrap_or(0);
    if version != 1 {
        return Err(format!("Wwise Opus version {version}"));
    }
    if ch == 0 || ch > 8 {
        return Err(format!("{ch} channels"));
    }
    let seek = r.seek.ok_or("no seek table")?;
    if seek.len() < count * 2 {
        return Err("seek table shorter than the frame count".into());
    }
    let mut packets = Vec::with_capacity(count);
    let mut at = 0usize;
    for i in 0..count {
        let n = usize::from(u16_at(seek, i * 2).unwrap_or(0));
        let p = r
            .data
            .get(at..at + n)
            .ok_or(format!("frame {i} runs past the data"))?;
        packets.push(p);
        at += n;
    }
    if packets.is_empty() {
        return Err("no frames".into());
    }
    ogg_opus_from_packets(
        ch as u8,
        skip,
        48_000,
        mapping,
        Some(u64::from(samples)),
        &packets,
    )
}

#[cfg(test)]
mod tests;
