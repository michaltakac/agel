use agel_core::{Value, World};

#[test]
fn clicks_require_an_observed_interface_and_encode_revision_241_actions() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../crypto.agel"),
        include_str!("../osrs-wire.agel"),
        include_str!("../osrs-crypto.agel"),
        include_str!("../osrs-packets.agel"),
        include_str!("../osrs-bits.agel"),
        include_str!("../osrs-npcs.agel"),
        include_str!("../osrs-game.agel"),
    ] {
        world.evaluate(source).unwrap();
    }
    let options = agel_core::EvaluationOptions {
        budget: agel_core::Budget {
            fuel: 15_000_000,
            ..Default::default()
        },
        ..Default::default()
    };
    let result = world.evaluate_with(r#"
      (import agel/sequence)
      (import agel/crypto)
      (import agel/osrs-game)
      (def read-raw (fn (n prefix) (if (= n 0) (reverse prefix)
        (let ((byte (car raw))) (def raw (cdr raw)) (read-raw (- n 1) (cons byte prefix))))))
      (def wire-read (fn (n) (read-raw n nil)))
      (def wire-write (fn (bytes) (def sent bytes) (count bytes)))
      (def decode (fn (state opcode body)
        (let ((byte (+ opcode (get (isaac-bytes-next (get state 'in)) 'value))))
          (def raw (cons (- byte (* 256 (/ byte 256))) body))
          (get (osrs-next-packet state) 'state))))
      (def original (osrs-game-state '(1 2 3 4)))
      (def used (osrs-use-location original 2097 3188 3424))
      (def location-body (cdr sent))
      (def opened (decode used 93 '(1 2 3 4 1 184 0)))
      (def clicked (osrs-click-interface opened 312 22))
      (def button-body (cdr sent))
      (def moved (decode clicked 22 '(3 4 1 2 7 8 5 6)))
      (def closed (decode moved 45 '(6 5 8 7)))
      (def refused (with-handler (* problem) (get problem 'kind) (osrs-click-interface closed 312 22)))
      (def resynced (decode opened 116 '(0 11 0 42 0 1 0 1 0 3 1 57 0)))
      (list location-body (osrs-open-interface? opened 312) button-body
        (osrs-open-interface? moved 312) (osrs-open-interface? closed 312) refused
        (osrs-open-interface? resynced 42) (osrs-open-interface? resynced 313)
        (osrs-open-interface? resynced 312))
    "#, &options).unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("((96 13 177 8 0 12 116 0) #t (1 56 0 22) #t #f osrs/interface #t #t #f)")
    );
}
