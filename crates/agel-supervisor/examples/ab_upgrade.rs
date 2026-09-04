use agel_core::{Budget, Value};
use agel_image::ImageSession;
use agel_supervisor::{AbSupervisor, HealthCheck};

fn main() {
    let mut stable = ImageSession::new(16, Budget::default());
    stable
        .evaluate("(def scheduler (fn (load) (+ load 1)))")
        .unwrap();
    let mut proposed = stable.image().rebuild().unwrap();
    proposed
        .evaluate("(def scheduler (fn (load) (+ 2 load)))")
        .unwrap();

    let mut supervisor = AbSupervisor::new(stable.image().clone());
    let evidence = supervisor
        .stage(
            proposed.image().clone(),
            &[
                HealthCheck::new("(scheduler 40)", Value::Int(42)),
                HealthCheck::new("(scheduler -2)", Value::Int(0)),
            ],
        )
        .expect("candidate passes isolated health checks");
    println!(
        "active {:?}: {}",
        supervisor.active_slot(),
        evidence.active_digest()
    );
    println!(
        "candidate: {} ({} checks)",
        evidence.candidate_digest(),
        evidence.checks_passed()
    );
    println!("promoted slot {:?}", supervisor.promote(&evidence).unwrap());
    println!(
        "watchdog rollback -> slot {:?}",
        supervisor.rollback().unwrap()
    );
}
