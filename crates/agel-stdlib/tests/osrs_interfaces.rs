use agel_core::{Value, World};

#[test]
fn cook_dialogue_retries_the_talk_after_a_missing_first_page() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(
            r#"(module agel/osrs-mining-test)
               (module agel/osrs-game
                 (export osrs-stat osrs-inventory-count osrs-await osrs-nearby-npcs
                         osrs-varp osrs-open-interface? osrs-interface-opens
                         osrs-await-bounded osrs-keepalive osrs-resume-dialogue
                         osrs-talk osrs-walk osrs-wait-ticks)
                 (def osrs-stat (fn (state _) (dict 'xp (if (= (get state 'page) 3) 300 0))))
                 (def osrs-inventory-count (fn (state inv item) (if (= (get state 'page) 3) 0 1)))
                 (def osrs-await (fn (state ready) state))
                 (def osrs-await-bounded
                   (fn (state ready ticks) (if (ready state) state (signal 'test/wait "missing page"))))
                 (def osrs-nearby-npcs (fn (state) (list (dict 'type 4626 'index 7))))
                 (def osrs-varp (fn (state varp) (if (= (get state 'page) 3) 1 0)))
                 (def osrs-open-interface?
                   (fn (state id)
                     (if (= id 219)
                       (if (= (get state 'page) 1) #t (= (get state 'page) 2))
                       (if (= id 153) (= (get state 'page) 3) #f))))
                 (def osrs-interface-opens
                   (fn (state id) (if (= id 219) (get state 'page) 0)))
                 (def osrs-keepalive (fn (state) state))
                 (def osrs-walk (fn (state x z) (assoc state 'tile (list x z))))
                 (def osrs-wait-ticks (fn (state ticks) state))
                 (def osrs-talk
                   (fn (state index)
                     (if (= (get state 'talks) 0)
                       (if (= (get state 'tile) '(3208 3216)) (assoc state 'talks 1)
                         (signal 'test/talk "expected the north approach"))
                       (begin
                         (if (= (get state 'talks) 1) #t (signal 'test/talk "unexpected extra talk"))
                         (if (= (get state 'tile) '(3207 3215)) #t
                           (signal 'test/talk "expected a different approach on retry"))
                         (assoc (assoc state 'talks 2) 'page 1)))))
                 (def osrs-resume-dialogue
                   (fn (state id component option)
                     (if (= (list id component option) '(219 1 1))
                         (assoc state 'page (+ 1 (get state 'page)))
                         (signal 'test/dialogue "unexpected dialogue action")))))"#,
        )
        .unwrap();
    world
        .evaluate(include_str!("../../../examples/osrs/quest-test.agel"))
        .unwrap();
    let result = world
        .evaluate(
            "(import agel/osrs-quest-test)
             (def report (finish-cooks-assistant (dict 'page 0 'talks 0)))
             (list (get report 'passed) (get report 'choices) (get report 'quest-points))",
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(#t 2 1)")
    );
}

#[test]
fn revision_241_interfaces_and_ground_items_follow_server_observations() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    for source in [
        include_str!("../bytes.agel"),
        include_str!("../crypto.agel"),
        include_str!("../../../examples/osrs/osrs-wire.agel"),
        include_str!("../../../examples/osrs/osrs-crypto.agel"),
        include_str!("../../../examples/osrs/osrs-packets.agel"),
        include_str!("../../../examples/osrs/osrs-bits.agel"),
        include_str!("../../../examples/osrs/osrs-npcs.agel"),
        include_str!("../../../examples/osrs/osrs-game.agel"),
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
        (osrs-use-location-option original 24009 3226 3256 4)))
      (def climbed (osrs-use-location-option original 12965 3164 3307 3))
      (def third-location-body (cdr sent))
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
      (list location-body second-location-body invalid-option third-location-body take-body invalid-ground (osrs-open-interface? opened 312) button-body
        (osrs-open-interface? moved 312) (osrs-open-interface? closed 312) refused
        (osrs-open-interface? resynced 42) (osrs-open-interface? resynced 313)
        (osrs-open-interface? resynced 312) talk-body invalid-talk
        choice-body continue-body invalid-resume trade-body invalid-trade sell-body buy-body invalid-offset absent-item invalid-sale
        (osrs-interface-opens opened 312) (osrs-interface-opens reopened 312)
        (osrs-varp small 101) (osrs-varp large 29) (osrs-varp wide 29) (osrs-varp reset 29) (ground-observations original))
    "#, &options).unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("((96 13 177 8 0 12 116 0) (128 12 184 93 73 0 12 154) osrs/location (235 12 92 12 0 37 50 128) (0 12 154 7 152 229 12 128) osrs/item #t (1 56 0 22) #t #f osrs/interface #t #t #f (128 116 1 0) osrs/npc (2 0 22 0 56 1) (255 255 22 0 56 1) osrs/interface (244 1 0 128) osrs/npc (1 56 0 22 0 0 9 45 2) (1 56 0 22 0 1 9 45 2) osrs/interface osrs/interface osrs/interface 1 2 -1 -257 -9223372036854775807 nil (nil 1 3 0 0 3 nil nil))")
    );

    let cadence = world
        .evaluate_with(
            r#"
      (def now 0)
      (def start-service
        (fn (step initial)
          (def run-step step)
          (def progress initial)))
      (def stop-service (fn () progress))
      (def paced (assoc original 'maintenance (dict
        'start start-service 'stop stop-service 'clock-us (fn () now)
        'keepalive-interval-us 3000000)))
      (osrs-service-start paced)
      (def first-pulse (run-step progress))
      (def progress (get first-pulse 'state))
      (def now 2999999)
      (def skipped (run-step progress))
      (def now 3000000)
      (def second-pulse (run-step (get skipped 'state)))
      (def progress (get second-pulse 'state))
      (def stopped (osrs-service-stop paced))
      (def direct (osrs-keepalive original))
      (def first-byte sent)
      (def direct (osrs-keepalive direct))
      (list (= (get first-pulse 'bytes) first-byte)
        (= (get skipped 'bytes) nil)
        (= (get second-pulse 'bytes) sent)
        (= (get stopped 'out) (get direct 'out))
        (get stopped 'keepalives))
    "#,
            &options,
        )
        .unwrap();
    assert_eq!(
        cadence.values.last().map(Value::to_string).as_deref(),
        Some("(#t #t #t #t 2)")
    );
}

#[test]
fn melee_retargets_a_stalled_rat_and_reports_the_confirmed_target() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(
            r#"
      (module agel/osrs-game
        (export osrs-keepalive osrs-next-packet osrs-stat osrs-nearby-npcs osrs-attack
          osrs-walk osrs-open-interface? osrs-click-interface osrs-varp)
        (def rat-one (dict 'index 1 'type 2854))
        (def rat-two (dict 'index 2 'type 2854))
        (def osrs-keepalive (fn (state) state))
        (def osrs-next-packet
          (fn (state) (dict 'state state 'packet (dict 'name "PLAYER_INFO"))))
        (def osrs-stat
          (fn (state skill) (dict 'xp (get state skill) 'level 1)))
        (def osrs-nearby-npcs (fn (state) (get state 'npcs)))
        (def osrs-walk (fn (state x z) state))
        (def osrs-open-interface? (fn (state interface) #t))
        (def osrs-click-interface
          (fn (state interface component)
            (let ((clicked (assoc state 'clicks (cons component (get state 'clicks)))))
              (if (= (get state 'clicks) nil) clicked
                (assoc (assoc clicked 'stance (if (= component 10) 1 3))
                       'npcs (list rat-one rat-two))))))
        (def osrs-varp (fn (state varp) (get state 'stance)))
        (def osrs-attack
          (fn (state index)
            (let ((next (assoc state 'attempts (cons index (get state 'attempts)))))
              (if (= index 1) next
                (assoc (assoc (assoc next 'npcs (list rat-one))
                  (if (= (get state 'stance) 1) 2 1) 8)
                  3 (+ (get state 3) 2)))))))
      (module agel/osrs-combat-test
        (export test-rat-combat-bounded)
        (def test-rat-combat-bounded
          (fn (state await after-attack)
            (dict 'state (assoc (assoc state 0 8) 3 1156)
                  'report (dict 'target-index 1)))))
    "#,
        )
        .unwrap();
    world
        .evaluate(include_str!("../../../examples/osrs/combat-training.agel"))
        .unwrap();
    let result = world
        .evaluate(
            r#"
      (import agel/osrs-combat-training)
      (import agel/sequence)
      (def trained (train-rat-melee-result (dict 0 0 1 0 2 0 3 1154) 8 2 nil))
      (def report (get trained 'report))
      (list (get report 'passed) (get report 'defence-xp) (get report 'stance)
        (reverse (get (get trained 'state) 'clicks))
        (reverse (get (get trained 'state) 'attempts))
        (map (fn (phase) (get (car (get phase 'encounters)) 'target-index)) (get report 'phases)))
    "#,
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(#t 8 3 (10 18 10 18) (1 1 2 2) (1 2 2))")
    );
}
