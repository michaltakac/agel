use agel_core::{Budget, Value};
use agel_image::{Image, ImageSession, ImageStore};

fn main() {
    let path =
        std::env::temp_dir().join(format!("agel-portable-demo-{}.image", std::process::id()));
    let store = ImageStore::new(&path);
    let mut live = ImageSession::new(16, Budget::default());
    live.evaluate("(def identity '(agentic lisp))")
        .expect("commit live definition");
    live.evaluate("(def answer (+ 20 22))")
        .expect("commit live computation");
    let root = store.save(live.image(), None).expect("atomic save");

    let bytes = live.image().encode();
    let restored = Image::decode(&bytes)
        .expect("integrity-checked decode")
        .rebuild()
        .expect("deterministic rebuild");
    assert_eq!(restored.world().binding("answer"), Some(&Value::Int(42)));
    println!("saved {} committed inputs", live.image().len());
    println!("image root {root}");
    println!(
        "restored answer = {}",
        restored.world().binding("answer").unwrap()
    );
    println!("portable bytes = {}", bytes.len());

    let _ = std::fs::remove_file(path);
}
