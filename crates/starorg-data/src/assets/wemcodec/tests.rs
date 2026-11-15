//! Synthetic WEM fixtures for every codec: the headers are built here.

use super::*;

/// A RIFF WAVE from `(id, body)` chunks (word padded).
fn riff_of(chunks: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF\0\0\0\0WAVE");
    for (id, body) in chunks {
        b.extend_from_slice(*id);
        b.extend_from_slice(&(body.len() as u32).to_le_bytes());
        b.extend_from_slice(body);
        if body.len() & 1 == 1 {
            b.push(0);
        }
    }
    let n = (b.len() - 8) as u32;
    b[4..8].copy_from_slice(&n.to_le_bytes());
    b
}

fn fmt(tag: u16, ch: u16, rate: u32, align: u16, bits: u16, extra: &[u8]) -> Vec<u8> {
    let mut f = Vec::new();
    f.extend_from_slice(&tag.to_le_bytes());
    f.extend_from_slice(&ch.to_le_bytes());
    f.extend_from_slice(&rate.to_le_bytes());
    f.extend_from_slice(&(rate * u32::from(align.max(1))).to_le_bytes());
    f.extend_from_slice(&align.to_le_bytes());
    f.extend_from_slice(&bits.to_le_bytes());
    f.extend_from_slice(&(extra.len() as u16).to_le_bytes());
    f.extend_from_slice(extra);
    f
}

/// Wwise's own chunks, which a standard file drops.
fn wwise_junk() -> Vec<(&'static [u8; 4], Vec<u8>)> {
    vec![
        (b"hash", vec![7; 16]),
        (b"junk", vec![0; 3]),
        (b"akd ", vec![1; 16]),
    ]
}

/// (tag, channels, rate, bits, data) of a WAV we wrote.
fn read_wav(w: &[u8]) -> (u16, u16, u32, u16, Vec<u8>) {
    assert_eq!(&w[0..4], b"RIFF");
    assert_eq!(&w[8..16], b"WAVEfmt ");
    assert_eq!(
        u32::from_le_bytes(w[4..8].try_into().unwrap()) as usize,
        w.len() - 8
    );
    let flen = u32::from_le_bytes(w[16..20].try_into().unwrap()) as usize;
    let f = &w[20..20 + flen];
    let tag = u16::from_le_bytes([f[0], f[1]]);
    let ch = u16::from_le_bytes([f[2], f[3]]);
    let rate = u32::from_le_bytes(f[4..8].try_into().unwrap());
    let bits = u16::from_le_bytes([f[14], f[15]]);
    let d = 20 + flen;
    assert_eq!(&w[d..d + 4], b"data");
    let n = u32::from_le_bytes(w[d + 4..d + 8].try_into().unwrap()) as usize;
    (tag, ch, rate, bits, w[d + 8..d + 8 + n].to_vec())
}

#[test]
fn codec_names_and_formats() {
    assert_eq!(Codec::from_tag(0xFFFF), Codec::Vorbis);
    assert_eq!(Codec::from_tag(0x0001), Codec::Pcm);
    assert_eq!(Codec::from_tag(0xFFFE), Codec::Pcm);
    assert_eq!(Codec::from_tag(0x0002), Codec::ImaAdpcm);
    assert_eq!(Codec::from_tag(0x0069), Codec::ImaAdpcm);
    assert_eq!(Codec::from_tag(0x3040), Codec::Opus);
    assert_eq!(Codec::from_tag(0x3041), Codec::WwiseOpus);
    assert_eq!(Codec::from_tag(0x1234).to_string(), "fmt 0x1234");
    assert_eq!(Codec::Vorbis.format(), Some(Format::Ogg));
    assert_eq!(Codec::Pcm.format().map(Format::ext), Some("wav"));
    assert_eq!(Codec::WwiseOpus.format().map(Format::ext), Some("opus"));
    assert_eq!(Codec::Plugin.format(), None);
    assert_eq!(mime_of("opus"), "audio/ogg");
    assert_eq!(mime_of("wav"), "audio/wav");
    assert_eq!(mime_of("wem"), "");
}

#[test]
fn pcm_becomes_a_plain_wav() {
    let pcm: Vec<u8> = (0..400i16).flat_map(|s| (s * 50).to_le_bytes()).collect();
    let mut chunks = vec![(b"fmt ", fmt(0x0001, 2, 48_000, 4, 16, &[]))];
    chunks.extend(wwise_junk());
    chunks.push((b"data", pcm.clone()));
    let c = convert(&riff_of(&chunks)).unwrap();
    assert_eq!((c.codec, c.format), (Codec::Pcm, Format::Wav));
    let (tag, ch, rate, bits, data) = read_wav(&c.bytes);
    assert_eq!((tag, ch, rate, bits), (1, 2, 48_000, 16));
    assert_eq!(data, pcm);
}

#[test]
fn wwise_extensible_pcm_keeps_its_channel_mask() {
    // Wwise's 0xFFFE fmt: 0x18 bytes, valid bits + channel mask, no GUID.
    let mut extra = Vec::new();
    extra.extend_from_slice(&16u16.to_le_bytes());
    extra.extend_from_slice(&0x3Fu32.to_le_bytes());
    let pcm = vec![0u8; 6 * 2 * 10 + 3]; // a torn last frame is dropped
    let w = riff_of(&[
        (b"fmt ", fmt(0xFFFE, 6, 48_000, 12, 16, &extra)),
        (b"data", pcm),
    ]);
    let c = convert(&w).unwrap();
    let (tag, ch, _, bits, data) = read_wav(&c.bytes);
    assert_eq!((tag, ch, bits, data.len()), (0xFFFE, 6, 16, 120));
    let mask = u32::from_le_bytes(c.bytes[40..44].try_into().unwrap());
    assert_eq!(mask, 0x3F);
}

/// A plain IMA encoder (same step tables), for round trips.
fn ima_encode(samples: &[i16], channels: usize) -> Vec<u8> {
    let frames = samples.len() / channels / IMA_SAMPLES;
    let mut out = Vec::new();
    for b in 0..frames {
        for c in 0..channels {
            let at = |i: usize| i32::from(samples[(b * IMA_SAMPLES + i) * channels + c]);
            let mut hist = at(0);
            let mut index = 0i32;
            out.extend_from_slice(&(hist as i16).to_le_bytes());
            out.push(index as u8);
            out.push(0);
            let mut nib = Vec::with_capacity(64);
            for i in 1..IMA_SAMPLES {
                let step = IMA_STEP[index as usize];
                let diff = at(i) - hist;
                let mut code = 0u8;
                let mut d = diff.abs();
                if diff < 0 {
                    code |= 8;
                }
                if d >= step {
                    code |= 4;
                    d -= step;
                }
                if d >= step / 2 {
                    code |= 2;
                    d -= step / 2;
                }
                if d >= step / 4 {
                    code |= 1;
                }
                ima_step(code, &mut hist, &mut index);
                nib.push(code);
            }
            nib.push(0); // the skipped last nibble
            out.extend(nib.chunks(2).map(|p| p[0] | (p[1] << 4)));
        }
    }
    out
}

#[test]
fn ima_adpcm_decodes_to_pcm_wav() {
    let ch = 2usize;
    let n = IMA_SAMPLES * 5;
    let src: Vec<i16> = (0..n)
        .flat_map(|i| {
            let t = i as f64 / 48_000.0;
            let l = (8000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
            [l, -l / 2]
        })
        .collect();
    let data = ima_encode(&src, ch);
    assert_eq!(data.len(), 5 * IMA_BLOCK * ch);
    let w = riff_of(&[
        (
            b"fmt ",
            fmt(0x0002, 2, 48_000, (IMA_BLOCK * ch) as u16, 4, &[0x41, 0]),
        ),
        (b"data", data),
    ]);
    let c = convert(&w).unwrap();
    assert_eq!((c.codec, c.format), (Codec::ImaAdpcm, Format::Wav));
    let (tag, chn, rate, bits, pcm) = read_wav(&c.bytes);
    assert_eq!((tag, chn, rate, bits), (1, 2, 48_000, 16));
    let got: Vec<i16> = pcm
        .chunks(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();
    assert_eq!(got.len(), src.len());
    // Header samples are exact; the rest track the source closely.
    assert_eq!(got[0], src[0]);
    assert_eq!(got[IMA_SAMPLES * ch + 1], src[IMA_SAMPLES * ch + 1]);
    // Every block restarts at step index 0, so the first nibbles lag; the mean stays small.
    let err: i64 = got
        .iter()
        .zip(&src)
        .map(|(a, b)| i64::from((i32::from(*a) - i32::from(*b)).abs()))
        .sum();
    let mean = err / got.len() as i64;
    assert!(mean < 400, "mean error {mean}");
}

#[test]
fn ima_with_odd_blocks_is_named() {
    let w = riff_of(&[
        (b"fmt ", fmt(0x0002, 1, 48_000, 0x200, 4, &[])),
        (b"data", vec![0; 0x400]),
    ]);
    let e = convert(&w).unwrap_err();
    assert_eq!(e.codec, Some(Codec::ImaAdpcm));
    assert!(
        e.message()
            .starts_with("IMA ADPCM audio that could not be converted"),
        "{e}"
    );
}

/// Ogg packets of a stream (with granules).
fn ogg_packets(b: &[u8]) -> Vec<(Vec<u8>, u64)> {
    let mut r = ogg::reading::PacketReader::new(std::io::Cursor::new(b));
    let mut v = Vec::new();
    while let Some(p) = r.read_packet().unwrap() {
        v.push((p.data.clone(), p.absgp_page()));
    }
    v
}

/// Fake CELT packets: TOC 0xFC = config 31 (20 ms), stereo, one frame -> 960 samples.
fn opus_packets(n: usize) -> Vec<Vec<u8>> {
    (0..n).map(|i| vec![0xFC, i as u8, 1, 2, 3]).collect()
}

#[test]
fn opus_packet_durations() {
    assert_eq!(opus_packet_samples(&[0xFC]), Some(960));
    assert_eq!(opus_packet_samples(&[0x08]), Some(960)); // SILK 20 ms
    assert_eq!(opus_packet_samples(&[0x79]), Some(1920)); // hybrid 20 ms x2
    assert_eq!(opus_packet_samples(&[0xFB, 0x03]), Some(2880)); // CELT 20 ms, 3 frames
    assert_eq!(opus_packet_samples(&[]), None);
}

#[test]
fn wwise_opus_becomes_ogg_opus() {
    let packets = opus_packets(120);
    let samples = 120 * 960 - 500;
    let mut extra = vec![0u8; 0x12];
    extra[0x18 - 18..0x1C - 18].copy_from_slice(&(samples as u32).to_le_bytes());
    extra[0x1C - 18..0x20 - 18].copy_from_slice(&120u32.to_le_bytes());
    extra[0x20 - 18..0x22 - 18].copy_from_slice(&312u16.to_le_bytes());
    extra[0x22 - 18] = 1;
    extra[0x23 - 18] = 0;
    let seek: Vec<u8> = packets
        .iter()
        .flat_map(|p| (p.len() as u16).to_le_bytes())
        .collect();
    let mut chunks = vec![(b"fmt ", fmt(0x3041, 2, 48_000, 0, 0, &extra))];
    chunks.extend(wwise_junk());
    chunks.push((b"seek", seek));
    chunks.push((b"data", packets.concat()));
    let c = convert(&riff_of(&chunks)).unwrap();
    assert_eq!((c.codec, c.format), (Codec::WwiseOpus, Format::Opus));
    let got = ogg_packets(&c.bytes);
    assert_eq!(got.len(), 2 + 120);
    let head = &got[0].0;
    assert_eq!(&head[..8], b"OpusHead");
    assert_eq!((head[8], head[9]), (1, 2));
    assert_eq!(u16::from_le_bytes([head[10], head[11]]), 312);
    assert_eq!(u32::from_le_bytes(head[12..16].try_into().unwrap()), 48_000);
    assert_eq!(head[18], 0);
    assert_eq!(&got[1].0[..8], b"OpusTags");
    for (k, p) in packets.iter().enumerate() {
        assert_eq!(&got[2 + k].0, p);
    }
    assert_eq!(got.last().unwrap().1, samples as u64 + 312);
}

#[test]
fn wwise_opus_without_its_table_is_named() {
    let mut extra = vec![0u8; 0x12];
    extra[0x22 - 18] = 1;
    extra[0x1C - 18] = 3;
    let w = riff_of(&[
        (b"fmt ", fmt(0x3041, 2, 48_000, 0, 0, &extra)),
        (b"data", vec![0xFC; 30]),
    ]);
    let e = convert(&w).unwrap_err();
    assert_eq!(e.codec, Some(Codec::WwiseOpus));
    assert!(e.reason.contains("seek"), "{e}");
}

#[test]
fn ogg_opus_wem_is_unwrapped() {
    let packets = opus_packets(10);
    let refs: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
    let ogg = ogg_opus_from_packets(2, 312, 48_000, 0, None, &refs).unwrap();
    let mut meta = vec![0u8; 8];
    meta[4..8].copy_from_slice(&4u32.to_le_bytes());
    let mut data = vec![9u8; 4];
    data.extend_from_slice(&ogg);
    let w = riff_of(&[
        (b"fmt ", fmt(0x3040, 2, 48_000, 0, 0, &[0; 0x12])),
        (b"meta", meta),
        (b"data", data),
    ]);
    let c = convert(&w).unwrap();
    assert_eq!((c.codec, c.format), (Codec::Opus, Format::Opus));
    assert_eq!(c.bytes, ogg);
    assert_eq!(ogg_packets(&c.bytes).last().unwrap().1, 10 * 960);
}

#[test]
fn multichannel_opus_uses_family_one() {
    let packets = opus_packets(2);
    let refs: Vec<&[u8]> = packets.iter().map(Vec::as_slice).collect();
    let ogg = ogg_opus_from_packets(6, 0, 48_000, 1, None, &refs).unwrap();
    let head = &ogg_packets(&ogg)[0].0;
    assert_eq!(&head[18..21], &[1, 4, 2]);
    assert_eq!(&head[21..27], &[0, 4, 1, 2, 3, 5]);
    assert!(ogg_opus_from_packets(9, 0, 48_000, 1, None, &refs).is_err());
}

#[test]
fn vorbis_check_accepts_a_real_stream_and_rejects_garbage() {
    let tone = include_bytes!("tone.ogg");
    assert!(check_ogg_vorbis(tone).unwrap() > 1000);
    assert!(check_ogg_vorbis(b"OggS not really").is_err());
}

#[test]
fn broken_vorbis_wem_names_the_codec() {
    let w = riff_of(&[
        (b"fmt ", fmt(0xFFFF, 2, 48_000, 0, 0, &[0; 0x30])),
        (b"data", vec![0; 64]),
    ]);
    let e = convert(&w).unwrap_err();
    assert_eq!(e.codec, Some(Codec::Vorbis));
    assert!(
        e.message()
            .starts_with("Vorbis audio that could not be converted"),
        "{e}"
    );
    assert!(!e.message().contains("Not Vorbis"));
}

#[test]
fn plugin_media_and_unknown_codecs_are_named() {
    let e = convert(b"PLUG\x01\x00\x00\x00 convolution IR").unwrap_err();
    assert_eq!(e.codec, Some(Codec::Plugin));
    assert!(e.message().contains("not audio"), "{e}");
    let w = riff_of(&[
        (b"fmt ", fmt(0x8311, 2, 48_000, 0, 4, &[])),
        (b"data", vec![0; 8]),
    ]);
    let e = convert(&w).unwrap_err();
    assert_eq!(e.codec, Some(Codec::Other(0x8311)));
    assert_eq!(
        e.message(),
        "PTADPCM (fmt 0x8311) audio cannot be converted; it is kept as .wem"
    );
    let e = convert(b"garbage that is no wem").unwrap_err();
    assert_eq!(e.codec, None);
    assert!(e.message().starts_with("Not a WEM file"));
}
