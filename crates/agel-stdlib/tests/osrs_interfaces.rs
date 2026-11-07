use agel_core::{Value, World};

#[test]
fn revision_241_interfaces_and_ground_items_follow_server_observations() {
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
      (def ground-observations
        (fn (start)
          (let ((base (decode start 96 '(0 8 255 255 1 146 146 1 0 0))))
            (let ((added (decode base 107 '(0 27 40 40 128
              11 128 31 128 255 0 65 0 0 10
              9 0 106 139 7 0 0 0 0 0 0 1 0 0 0))))
              (let ((updated (decode added 107 '(0 15 40 40 128
                2 150 11 7 0 0 3 0 0 0 0 1))))
                (let ((removed (decode updated 107 '(0 11 40 40 128
                  5 139 7 234 0 0 0 3)))
                      (cleared (decode updated 68 '(40 0 40)))
                      (overlap (decode updated 96 '(0 6 1 147 147 1 0 0)))
                      (distant (decode updated 96 '(0 6 1 170 170 1 0 0)))
                      (instance (decode updated 39 '(0 0))))
                  (map (fn (state) (osrs-ground-count state 1931 3209 3214 0))
                    (list base added updated removed cleared overlap distant instance))))))))
      (def original (osrs-game-state '(1 2 3 4)))
      (def used (osrs-use-location original 2097 3188 3424))
      (def location-body (cdr sent))
      (def operated (osrs-use-location-option original 24009 3226 3256 2))
      (def second-location-body (cdr sent))
      (def invalid-option (with-handler (* problem) (get problem 'kind)
        (osrs-use-location-option original 24009 3226 3256 3)))
      (def gathered (osrs-take-ground-item original 1944 3226 3301))
      (def take-body (cdr sent))
      (def invalid-ground (with-handler (* problem) (get problem 'kind)
        (osrs-take-ground-item original 65535 3226 3301)))
      (def opened (decode used 93 '(1 2 3 4 1 184 0)))
      (def clicked (osrs-click-interface opened 312 22))
      (def button-body (cdr sent))
      (def moved (decode clicked 22 '(3 4 1 2 7 8 5 6)))
      (def closed (decode moved 45 '(6 5 8 7)))
      (def refused (with-handler (* problem) (get problem 'kind) (osrs-click-interface closed 312 22)))
      (def resynced (decode opened 116 '(0 11 0 42 0 1 0 1 0 3 1 57 0)))
      (def tutor-state (assoc original 'npc-worlds (dict 0 (dict 'npcs (dict 500 (dict 'type 3222))))))
      (def talked (osrs-talk tutor-state 500))
      (def talk-body (cdr sent))
      (def invalid-talk (with-handler (* problem) (get problem 'kind) (osrs-talk original 500)))
      (def traded (osrs-trade tutor-state 500))
      (def trade-body (cdr sent))
      (def invalid-trade (with-handler (* problem) (get problem 'kind) (osrs-trade original 500)))
      (def stocked (assoc opened 'inventories
        (dict 93 (dict 0 (dict 'item 2349 'count 1)))))
      (def sold (osrs-click-inventory-item stocked 312 22 93 2349 2))
      (def sell-body (cdr sent))
      (def bought (osrs-click-inventory-item-offset stocked 312 22 93 2349 2 1))
      (def buy-body (cdr sent))
      (def invalid-offset (with-handler (* problem) (get problem 'kind)
        (osrs-click-inventory-item-offset stocked 312 22 93 2349 2 65535)))
      (def absent-item (with-handler (* problem) (get problem 'kind)
        (osrs-click-inventory-item stocked 312 22 93 995 2)))
      (def invalid-sale (with-handler (* problem) (get problem 'kind)
        (osrs-click-inventory-item closed 312 22 93 2349 2)))
      (def choice (osrs-resume-dialogue opened 312 22 2))
      (def choice-body (cdr sent))
      (def continued (osrs-resume-dialogue opened 312 22 -1))
      (def continue-body (cdr sent))
      (def invalid-resume (with-handler (* problem) (get problem 'kind)
        (osrs-resume-dialogue closed 312 22 -1)))
      (def reopened (decode opened 93 '(1 2 3 4 1 184 0)))
      (def small (decode reopened 21 '(129 101 0)))
      (def large (decode small 40 '(157 0 255 255 255 254)))
      (def wide (decode large 24 '(0 128 0 0 0 0 1 0 0 29)))
      (def reset (decode wide 30 nil))
      (list location-body second-location-body invalid-option take-body invalid-ground (osrs-open-interface? opened 312) button-body
        (osrs-open-interface? moved 312) (osrs-open-interface? closed 312) refused
        (osrs-open-interface? resynced 42) (osrs-open-interface? resynced 313)
        (osrs-open-interface? resynced 312) talk-body invalid-talk
        choice-body continue-body invalid-resume trade-body invalid-trade sell-body buy-body invalid-offset absent-item invalid-sale
        (osrs-interface-opens opened 312) (osrs-interface-opens reopened 312)
        (osrs-varp small 101) (osrs-varp large 29) (osrs-varp wide 29) (osrs-varp reset 29) (ground-observations original))
    "#, &options).unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("((96 13 177 8 0 12 116 0) (128 12 184 93 73 0 12 154) osrs/location (0 12 154 7 152 229 12 128) osrs/item #t (1 56 0 22) #t #f osrs/interface #t #t #f (128 116 1 0) osrs/npc (2 0 22 0 56 1) (255 255 22 0 56 1) osrs/interface (244 1 0 128) osrs/npc (1 56 0 22 0 0 9 45 2) (1 56 0 22 0 1 9 45 2) osrs/interface osrs/interface osrs/interface 1 2 -1 -257 -9223372036854775807 nil (nil 1 3 0 0 3 nil nil))")
    );
}
