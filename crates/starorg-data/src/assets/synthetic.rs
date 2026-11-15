//! A small synthetic `Data.p4k` (a plain stored ZIP whose `crc32` fields are CRC-32C, as
//! CIG's are) for the asset worker tests: a DataCore, a texture split into a base file and a
//! mip sibling, XML, a CryXmlB-free text file, a non-Vorbis WEM and some folders. Test
//! builds only (`synthetic-dcb`).

use std::path::Path;

/// `width` x `height` RGBA8 uncompressed DDS with `mips` levels; the base file holds the
/// smaller mips, `.1` the largest (CryEngine's split layout).
pub fn dds_split(width: u32, height: u32) -> (Vec<u8>, Vec<u8>) {
    let mut h = Vec::new();
    h.extend_from_slice(b"DDS ");
    let w32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    w32(&mut h, 124);
    w32(&mut h, 0x1 | 0x2 | 0x4 | 0x1000 | 0x20000);
    w32(&mut h, height);
    w32(&mut h, width);
    w32(&mut h, width * 4);
    w32(&mut h, 0);
    w32(&mut h, 2); // mips
    for _ in 0..11 {
        w32(&mut h, 0);
    }
    // Pixel format: RGB + alpha, 32 bits, RGBA masks.
    w32(&mut h, 32);
    w32(&mut h, 0x40 | 0x1);
    w32(&mut h, 0);
    w32(&mut h, 32);
    w32(&mut h, 0x0000_00ff);
    w32(&mut h, 0x0000_ff00);
    w32(&mut h, 0x00ff_0000);
    w32(&mut h, 0xff00_0000);
    w32(&mut h, 0x1000 | 0x400000 | 0x8);
    for _ in 0..4 {
        w32(&mut h, 0);
    }
    let mip = |w: u32, h: u32, v: u8| -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| [v, (i % 256) as u8, 255 - v, 255])
            .collect()
    };
    let mut base = h;
    base.extend(mip((width / 2).max(1), (height / 2).max(1), 40));
    (base, mip(width, height, 200))
}

/// A RIFF/WAVE WEM with a PCM `fmt ` chunk: parsed as WEM, not Vorbis (kept as `.wem`).
pub fn pcm_wem(samples: usize) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&1u16.to_le_bytes()); // mono
    fmt.extend_from_slice(&48_000u32.to_le_bytes());
    fmt.extend_from_slice(&96_000u32.to_le_bytes());
    fmt.extend_from_slice(&2u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let data = vec![0u8; samples * 2];
    let mut body = Vec::new();
    body.extend_from_slice(b"WAVE");
    body.extend_from_slice(b"fmt ");
    body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    body.extend_from_slice(&fmt);
    body.extend_from_slice(b"data");
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(&data);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// A stored ZIP of `files` (name, bytes), names backslash separated.
pub fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let u16le = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_le_bytes());
    let u32le = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
    for (name, data) in files {
        let crc = crc32c::crc32c(data);
        let off = out.len() as u32;
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u16le(&mut out, 0x5821); // 2024-01-01
        u32le(&mut out, crc);
        u32le(&mut out, data.len() as u32);
        u32le(&mut out, data.len() as u32);
        u16le(&mut out, name.len() as u16);
        u16le(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 20);
        u16le(&mut central, 20);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0x5821);
        u32le(&mut central, crc);
        u32le(&mut central, data.len() as u32);
        u32le(&mut central, data.len() as u32);
        u16le(&mut central, name.len() as u16);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u32le(&mut central, 0);
        u32le(&mut central, off);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    u16le(&mut out, files.len() as u16);
    u16le(&mut out, files.len() as u16);
    u32le(&mut out, central.len() as u32);
    u32le(&mut out, cd_off);
    u16le(&mut out, 0);
    out
}

/// The texture in the synthetic archive.
pub const TEXTURE: &str = "Data\\Textures\\ships\\syn_hull_diff.dds";
/// The XML file.
pub const XML: &str = "Data\\Scripts\\syn_config.xml";
/// The loose WEM.
pub const WEM: &str = "Data\\Sounds\\wwise\\Media\\123456.wem";

/// A GameAudio document with two ATL triggers (the Explorer's Sounds search, 7.17).
pub const GAME_AUDIO: &str = "Data\\Libs\\GameAudio\\syn_audio.xml";
pub const GAME_AUDIO_XML: &str = "<ATLConfig><AudioTriggers>\
<ATLTrigger atl_name=\"Play_SYN_Engine_Loop\"><WwiseEvent wwise_name=\"Play_SYN_Engine_Loop\" \
wwise_duration_type=\"Infinite\"/></ATLTrigger>\
<ATLTrigger atl_name=\"Play_SYN_Explosion_Big\"><WwiseEvent wwise_name=\"Play_SYN_Boom\"/></ATLTrigger>\
</AudioTriggers></ATLConfig>";

/// Every file of the synthetic archive.
pub fn files() -> Vec<(&'static str, Vec<u8>)> {
    let (base, mip1) = dds_split(8, 8);
    vec![
        (
            "Data\\Game2.dcb",
            crate::starbreaker::synthetic::build(3, 10, 4),
        ),
        (TEXTURE, base),
        ("Data\\Textures\\ships\\syn_hull_diff.dds.1", mip1),
        (
            XML,
            b"<Config name=\"syn\">\n  <Value key=\"speed\" v=\"200\"/>\n</Config>\n".to_vec(),
        ),
        ("Data\\Scripts\\readme.txt", b"plain text".to_vec()),
        ("Data\\Objects\\ships\\syn\\syn_hull.skin", vec![1u8; 300]),
        (
            "Data\\Objects\\ships\\syn\\syn_hull.mtl",
            b"<Material/>".to_vec(),
        ),
        (WEM, pcm_wem(480)),
        ("Data\\Sounds\\wwise\\bad.wem", b"not a wem".to_vec()),
    ]
}

/// The synthetic archive plus [`GAME_AUDIO`], for the trigger search.
pub fn write_with_audio(path: &Path) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut f = files();
    f.push((GAME_AUDIO, GAME_AUDIO_XML.as_bytes().to_vec()));
    std::fs::write(path, zip(&f))
}

/// Write the synthetic archive to `path`.
pub fn write(path: &Path) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, zip(&files()))
}
