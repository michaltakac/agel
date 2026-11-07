use agel_core::{Value, World};

#[test]
fn sealed_history_preserves_order_and_existing_snapshots() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(include_str!("../record-chain.agel"))
        .unwrap();
    let result = world
        .evaluate(
            r#"(import agel/record-chain)
            (def add (fn (history next end)
              (if (< end next) history (add (record-chain-push history next) (+ next 1) end))))
            (def snapshot (add nil 1 32))
            (def extended (add snapshot 33 257))
            (def legacy (dict 'size 9 'records '(9 8 7 6 5 4 3 2 1) 'previous nil))
            (def resumed (record-chain-push legacy 10))
            (def check-descending (fn (expected record)
              (if (= expected record) (- expected 1) (signal 'test "history is out of order"))))
            (list (record-chain-fold check-descending 32 snapshot)
                  (record-chain-fold check-descending 257 extended)
                  (record-chain-fold check-descending 10 resumed)
                  (record-chain-fold check-descending 0 nil))"#,
        )
        .unwrap();
    assert_eq!(
        result.values.last(),
        Some(&Value::List(vec![Value::Int(0); 4]))
    );
}
