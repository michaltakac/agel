use agel_effects::{CowWorkspace, EffectKind, Principal, StaticPolicy, WorkspaceBroker};

fn main() {
    let files = CowWorkspace::from_files([
        ("/system/scheduler.agel".into(), b"(def tick old)".to_vec()),
        ("/system/policy.agel".into(), b"(deny-by-default)".to_vec()),
    ])
    .expect("valid seed image");

    // Reads are allowed; writes are virtualized into the copy-on-write overlay.
    let policy = StaticPolicy::default()
        .allow(EffectKind::FileRead, "read")
        .virtualize(EffectKind::FileWrite);
    let mut broker = WorkspaceBroker::new(policy, files);
    let agent = Principal {
        world: 1,
        agent: Some(7),
    };
    let decision = broker
        .write(
            agent.clone(),
            "/system/scheduler.agel",
            b"(def tick proposed)".to_vec(),
        )
        .expect("isolated write");
    println!("write decision = {decision:?}");
    println!("base is untouched; proposed view = {:?}", broker.diff());
    broker.rollback();
    println!("after rollback = {:?}", broker.diff());
    // A default policy grants nothing: the same request is refused and audited.
    let mut sealed = WorkspaceBroker::new(StaticPolicy::default(), broker.workspace().clone());
    println!(
        "default policy: {}",
        sealed
            .delete(agent, "/system/policy.agel")
            .map(|_| "allowed".to_owned())
            .unwrap_or_else(|error| error.to_string())
    );
    for record in broker
        .audit_log()
        .records()
        .into_iter()
        .chain(sealed.audit_log().records())
    {
        println!(
            "#{} {:?} {} {} -> {:?}",
            record.sequence,
            record.intent.kind,
            record.intent.operation,
            record.intent.resource,
            record.outcome
        );
    }
}
