use super::*;
use crate::assets::synthetic;
use crate::starbreaker::synthetic as dcb;
use serde_json::json;

#[derive(Clone, Default)]
struct Sink(Arc<Mutex<Vec<u8>>>);

impl Write for Sink {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        lock(&self.0).extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Sink {
    fn replies(&self) -> Vec<Reply> {
        String::from_utf8_lossy(&lock(&self.0))
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    /// The final reply of request `id`, waiting up to 10 s.
    fn wait(&self, id: u64) -> Reply {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(r) = self
                .replies()
                .into_iter()
                .find(|r| r.id == id && r.progress.is_none())
            {
                return r;
            }
            assert!(Instant::now() < until, "no reply to {id}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

struct Running {
    _t: tempfile::TempDir,
    root: PathBuf,
    w: std::io::PipeWriter,
    sink: Sink,
    done: std::thread::JoinHandle<()>,
}

fn start(idle: Duration) -> Running {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().to_path_buf();
    let p = root.join("game").join("Data.p4k");
    synthetic::write(&p).unwrap();
    let a = Arc::new(AssetArchive::open(&p).unwrap());
    let server = Server::new(a, true);
    let (r, w) = std::io::pipe().unwrap();
    let sink = Sink::default();
    let out: Out = Arc::new(Mutex::new(Box::new(sink.clone())));
    let done = std::thread::spawn(move || {
        serve(&server, std::io::BufReader::new(r), out, idle);
    });
    Running {
        _t: t,
        root,
        w,
        sink,
        done,
    }
}

impl Running {
    fn send(&mut self, v: Value) {
        writeln!(self.w, "{v}").unwrap();
        self.w.flush().unwrap();
    }
    fn call(&mut self, id: u64, mut v: Value) -> Reply {
        v["id"] = json!(id);
        self.send(v);
        self.sink.wait(id)
    }
}

#[test]
fn answers_files_sounds_and_errors_then_exits_on_eof() {
    let mut w = start(IDLE_EXIT);
    let st = w.call(1, json!({"op": "status"}));
    let st: AssetStatus = serde_json::from_value(st.result.unwrap()).unwrap();
    assert!(!st.datacore && !st.audio, "lazy: {st:?}");
    let list = w.call(2, json!({"op": "list", "dir": "Data", "limit": 10}));
    let page: files::DirPage = serde_json::from_value(list.result.unwrap()).unwrap();
    assert_eq!(page.total, 5);
    let s = w.call(3, json!({"op": "search", "query": "dds", "limit": 10}));
    assert_eq!(s.result.unwrap()["total"], 2);
    let png = w.root.join("cache").join("p.png");
    let p = w.call(
        4,
        json!({"op": "preview", "path": synthetic::TEXTURE, "out": png}),
    );
    assert_eq!(p.result.unwrap()["kind"], "image");
    assert!(png.exists());
    let snd = w.call(5, json!({"op": "sounds", "what": dcb::guid_text(1, 4)}));
    assert_eq!(snd.result.unwrap()["name"], "SYN_Item_004");
    let one = w.call(
        6,
        json!({"op": "sound",
               "sound": {"media_id": 123456, "source": "media-file", "bank": ""},
               "out": w.root.join("a.wem")}),
    );
    let done: SoundDone = serde_json::from_value(one.result.unwrap()).unwrap();
    assert_eq!((done.ext.as_str(), done.codec.as_str()), ("wav", "PCM"));
    assert_eq!(done.note, None);
    assert!(w.root.join("a.wav").exists());
    let odd = w.call(
        61,
        json!({"op": "sound",
               "sound": {"media_id": 123457, "source": "media-file", "bank": ""},
               "out": w.root.join("b.wem")}),
    );
    let done: SoundDone = serde_json::from_value(odd.result.unwrap()).unwrap();
    assert_eq!(done.ext, "wem");
    assert_eq!(
        done.note.as_deref(),
        Some("PTADPCM (fmt 0x8311) audio cannot be converted; it is kept as .wem")
    );
    assert!(w.root.join("b.wem").exists());
    let st = w.call(7, json!({"op": "status"}));
    let st: AssetStatus = serde_json::from_value(st.result.unwrap()).unwrap();
    assert!(st.datacore && st.audio, "built on demand and kept");
    // Errors: unknown op, bad JSON, not found, a panic, a write into the game folder.
    let e = w.call(8, json!({"op": "nope"}));
    assert_eq!(e.error.unwrap().code, "invalid");
    w.send(json!("not an object"));
    let e = w.call(9, json!({"op": "list", "dir": "Nope", "limit": 1}));
    assert_eq!(e.error.unwrap().code, "not-found");
    let e = w.call(10, json!({"op": "test-panic"}));
    let e = e.error.unwrap();
    assert_eq!(e.code, "internal");
    assert!(e.message.contains("panic"));
    let e = w.call(
        11,
        json!({"op": "preview", "path": synthetic::TEXTURE, "out": w.root.join("game").join("x.png")}),
    );
    assert!(e.error.unwrap().message.contains("exports never go there"));
    // Still alive after the panic.
    assert!(w.call(12, json!({"op": "status"})).ok);
    drop(w.w);
    w.done.join().unwrap();
}

#[test]
fn streams_extract_and_sound_exports_with_progress() {
    let mut w = start(IDLE_EXIT);
    let out = w.root.join("out");
    let r = w.call(
        1,
        json!({"op": "extract", "paths": ["Data\\Textures"], "out_dir": out, "convert": true}),
    );
    let d: files::ExtractDone = serde_json::from_value(r.result.unwrap()).unwrap();
    assert_eq!((d.files, d.converted, d.merged), (1, 1, 1));
    let progress: Vec<ExportProgress> = w
        .sink
        .replies()
        .into_iter()
        .filter(|r| r.id == 1)
        .filter_map(|r| r.progress)
        .collect();
    assert_eq!(progress.first().map(|p| (p.done, p.total)), Some((0, 2)));
    assert_eq!(progress.last().map(|p| (p.done, p.total)), Some((2, 2)));
    let r = w.call(
        2,
        json!({"op": "export-sounds",
               "sounds": [["Play_Thing", {"media_id": 123456, "source": "media-file", "bank": ""}],
                          ["Odd", {"media_id": 123457, "source": "media-file", "bank": ""}],
                          ["Gone", {"media_id": 7, "source": "unavailable", "bank": "b.bnk"}]],
               "out_dir": w.root.join("snd"), "wem": true}),
    );
    let d: SoundsDone = serde_json::from_value(r.result.unwrap()).unwrap();
    // The PCM sound as .wav plus its .wem; the PTADPCM one kept as .wem; one failure.
    assert_eq!((d.files, d.failures), (3, 1));
    let snd = w.root.join("snd");
    assert!(snd.join("Play_Thing_123456.wav").exists());
    assert!(snd.join("Play_Thing_123456.wem").exists());
    assert!(snd.join("Odd_123457.wem").exists());
    assert_eq!(SoundsDone::formats_line(&d.formats), "1 .wav, 1 .wem");
    assert_eq!(d.kept.len(), 1);
    assert!(d.kept[0].starts_with("Odd 123457: PTADPCM"), "{:?}", d.kept);
    let ext: Vec<_> = d.written.iter().map(|w| w.ext.as_str()).collect();
    assert_eq!(ext, ["wav", "wem"]);
    // A model export of a non-entity fails cleanly, as a streamed job.
    let r = w.call(
        3,
        json!({"op": "export-model", "what": dcb::guid_text(1, 1),
               "options": {"preset": "print-shell", "lod": 0}, "out": w.root.join("m.glb")}),
    );
    assert_eq!(r.error.unwrap().code, "invalid");
    // Cancel of an unknown job answers false.
    let c = w.call(4, json!({"op": "cancel", "job": 99}));
    assert_eq!(c.result, Some(Value::Bool(false)));
    drop(w.w);
    w.done.join().unwrap();
}

#[test]
fn exits_when_idle() {
    let w = start(Duration::from_secs(1));
    let t = Instant::now();
    w.done.join().unwrap();
    assert!(t.elapsed() < Duration::from_secs(5));
    drop(w.w);
}

#[test]
fn args() {
    let a = |v: &[&str]| parse_args(&v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>());
    let x = a(&["--p4k", "/g/Data.p4k", "--idle-secs", "30"]).unwrap();
    assert_eq!(x.p4k, PathBuf::from("/g/Data.p4k"));
    assert_eq!(x.idle, Duration::from_secs(30));
    assert!(!x.test_panic);
    assert!(a(&[]).is_err());
    assert!(a(&["--p4k", "x", "--bogus"]).is_err());
    assert!(a(&["--p4k", "x", "--idle-secs", "n"]).is_err());
}
