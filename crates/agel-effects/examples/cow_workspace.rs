use agel_effects::CowWorkspace;

fn main() {
    let mut files = CowWorkspace::from_files([
        ("/system/scheduler.agel".into(), b"(def tick old)".to_vec()),
        ("/system/policy.agel".into(), b"(deny-by-default)".to_vec()),
    ])
    .expect("valid seed image");

    files
        .write("/system/scheduler.agel", b"(def tick proposed)".to_vec())
        .expect("isolated write");
    println!("base is untouched; proposed view = {:?}", files.diff());
    files.rollback();
    println!("after rollback = {:?}", files.diff());
}
