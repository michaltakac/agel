use agel_core::{Value, World};

#[test]
fn mining_tutor_waits_for_a_new_actionable_menu() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(
            r#"
      (module agel/osrs-game
        (export osrs-open-interface? osrs-interface-opens osrs-pause-events
                osrs-map-route osrs-nearby-npcs osrs-talk osrs-keepalive osrs-observe-bounded)
        (def osrs-map-route (fn (state) (get state 'route)))
        (def osrs-nearby-npcs (fn (_) (list (dict 'type 3222 'index 7))))
        (def osrs-talk (fn (state _) (assoc state 'talks (+ 1 (get state 'talks)))))
        (def osrs-keepalive (fn (state) state))
        (def osrs-observe-bounded
          (fn (state _ready _ticks)
            (let ((window (+ 1 (get state 'window))))
              (let ((next (assoc (assoc state 'window window) 'route (dict 'sequence 2))))
                (if (= window 14) (assoc (assoc (assoc next 'open #t) 'opens 2) 'pauses 2)
                  next)))))
        (def osrs-open-interface? (fn (state _) (get state 'open)))
        (def osrs-interface-opens (fn (state _) (get state 'opens)))
        (def osrs-pause-events (fn (state _id _component) (get state 'pauses))))"#,
        )
        .unwrap();
    let source = include_str!("../../../examples/osrs/mining-test.agel").replace(
        "(export mine-first-copper",
        "(export fresh-tutor-menu? talk-to-tutor mine-first-copper",
    );
    world.evaluate(&source).unwrap();
    let result = world
        .evaluate(
            r#"
      (import agel/osrs-mining-test)
      (def stale (dict 'open #t 'opens 1 'pauses 1))
      (def opened (assoc stale 'opens 2))
      (def enabled (assoc opened 'pauses 2))
      (def before (assoc (assoc (assoc stale 'route (dict 'sequence 1)) 'window 0) 'talks 0))
      (def reply (talk-to-tutor before (fn (_) nil)))
      (list (fresh-tutor-menu? stale stale) (fresh-tutor-menu? opened stale)
            (fresh-tutor-menu? enabled stale) (fresh-tutor-menu? (assoc enabled 'open #f) stale)
            (get reply 'fresh) (get (get reply 'state) 'talks) (get (get reply 'state) 'window))"#,
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(#f #f #t #f #t 1 14)")
    );
}

#[test]
fn mining_advice_retries_only_after_a_fresh_menu() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world.evaluate(r#"
      (module agel/osrs-game
        (export osrs-open-interface? osrs-interface-opens osrs-pause-events osrs-map-route
          osrs-nearby-npcs osrs-talk osrs-resume-dialogue osrs-keepalive osrs-observe-bounded osrs-observe-ready)
        (def osrs-open-interface? (fn (state id) (= id (get state 'active))))
        (def osrs-interface-opens (fn (state id) (get state id)))
        (def osrs-pause-events (fn (state id _) (get state id)))
        (def osrs-map-route (fn (_) nil))
        (def osrs-nearby-npcs (fn (_) (list (dict 'type 3222 'index 7))))
        (def osrs-talk
          (fn (state _) (assoc (assoc (assoc state 219 2) 'active 219) 'talks 1)))
        (def osrs-resume-dialogue
          (fn (state id _component _slot)
            (if (= id 217)
              (if (get state 'reopened)
                (assoc (assoc state 'active 193) 193 1)
                (assoc (assoc (assoc state 'active 219) 219 2) 'reopened #t))
              (let ((choices (+ 1 (get state 'choices))))
                (if (= choices (get state 219))
                  (let ((next (assoc state 'choices choices)))
                    (if (= choices 2) (assoc (assoc next 'active 217) 217 (+ 1 (get state 217))) next))
                  (signal 'test "choice reused an old menu"))))))
        (def osrs-keepalive (fn (state) state))
        (def osrs-observe-bounded (fn (state _ready _ticks) state))
        (def osrs-observe-ready (fn (state _ready _limit) state)))
    "#).unwrap();
    let mining = include_str!("../../../examples/osrs/mining-test.agel").replace(
        "(export mine-first-copper",
        "(export advice-page advice-object mine-first-copper",
    );
    world.evaluate(&mining).unwrap();
    let result = world
        .evaluate(
            "(import agel/osrs-mining-test)
         (def page (advice-page (dict 'active 219 219 1 217 0 'choices 0 'talks 0) (fn (_) nil)))
         (def object (advice-object (dict 'active 217 217 1 219 1 193 0 'choices 1 'talks 0)
           (fn (_) nil) #t))
         (list (get page 'active) (get page 'talks) (get page 'choices)
               (get object 'active) (get object 'talks) (get object 'choices))",
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(217 1 2 193 0 2)")
    );
}

#[test]
fn cook_dialogue_retries_the_talk_after_a_missing_first_page() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(
            r#"(module agel/osrs-mining-test)
               (module agel/osrs-game
                 (export osrs-stat osrs-inventory-count osrs-await osrs-nearby-npcs
                         osrs-varp osrs-open-interface? osrs-interface-opens osrs-pause-events
                         osrs-await-bounded osrs-observe-bounded osrs-observe-ready osrs-keepalive osrs-resume-dialogue
                         osrs-talk osrs-walk osrs-use-location osrs-wait-ticks osrs-route-cleared?)
                 (def osrs-stat (fn (state _) (dict 'xp (if (= (get state 'page) 3) 300 0))))
                 (def osrs-inventory-count (fn (state inv item) (if (= (get state 'page) 3) 0 1)))
                 (def osrs-await (fn (state ready) state))
                 (def osrs-await-bounded
                   (fn (state ready ticks) (if (ready state) state (signal 'test/wait "missing page"))))
                 (def osrs-observe-bounded
                   (fn (state ready ticks)
                     (if (= ticks 40)
                       (let ((arrived (assoc state 'arrived #t)))
                         (if (ready arrived) arrived (signal 'test/route "arrival was missed")))
                       (if (= (get state 'talks) 2)
                         (if (get state 'waited)
                           (let ((opened (assoc state 'page 1)))
                             (if (ready opened) (signal 'test/wait "page was used before its pause event")
                               (let ((enabled (assoc opened 'enabled #t)))
                                 (if (ready enabled) enabled (signal 'test/wait "pause event was missed")))))
                           (assoc state 'waited #t))
                         state))))
                 (def osrs-observe-ready
                   (fn (state ready limit)
                     (if (get state 'late)
                       (let ((enabled (assoc (assoc state 'page 1) 'enabled #t)))
                         (if (ready enabled) enabled (signal 'test/wait "late page was missed")))
                       state)))
                 (def osrs-nearby-npcs (fn (state) (list (dict 'type 4626 'index 7))))
                 (def osrs-varp (fn (state varp) (if (= (get state 'page) 3) 1 0)))
                 (def osrs-open-interface?
                   (fn (state id)
                     (if (= id 219)
                       (if (= (get state 'page) 1) #t (= (get state 'page) 2))
                       (if (= id 153) (= (get state 'page) 3) #f))))
                 (def osrs-interface-opens
                   (fn (state id) (if (= id 219) (get state 'page) 0)))
                 (def osrs-pause-events
                   (fn (state id component)
                     (if (get state 'enabled) (osrs-interface-opens state id) 0)))
                 (def osrs-keepalive (fn (state) state))
                 (def osrs-walk
                   (fn (state x z)
                     (if (if (get state 'late) (< 0 (get state 'talks)) #f)
                       (signal 'test/route "late actionable page should prevent another walk")
                       (assoc (assoc state 'tile (list x z)) 'arrived #f))))
                 (def osrs-route-cleared?
                   (fn (state previous x z)
                     (if (get state 'arrived) (= (get state 'tile) (list x z)) #f)))
                 (def osrs-use-location
                   (fn (state id x z)
                     (let ((door (list id x z (get state 'tile) (get state 'door))))
                       (if (= door '(1529 3216 3218 (3221 3218) nil)) (assoc state 'door 'castle)
                         (if (= door '(12349 3213 3221 (3215 3221) castle)) (assoc state 'door 'passage)
                           (if (= door '(12348 3207 3217 (3207 3218) passage)) (assoc state 'door 'kitchen)
                             (signal 'test/route "expected the castle doors in order")))))))
                 (def osrs-wait-ticks
                   (fn (state ticks)
                     (if (= (get state 'talks) 2)
                       (signal 'test/wait "draining ticks would lose the first page") state)))
                 (def osrs-talk
                   (fn (state index)
                     (if (get state 'arrived) #t (signal 'test/route "talk before observed arrival"))
                     (if (= (get state 'talks) 0)
                       (if (= (list (get state 'tile) (get state 'door)) '((3208 3216) kitchen)) (assoc state 'talks 1)
                         (signal 'test/talk "expected the north approach"))
                       (begin
                         (if (= (get state 'talks) 1) #t (signal 'test/talk "unexpected extra talk"))
                         (if (= (get state 'tile) '(3207 3215)) #t
                           (signal 'test/talk "expected a different approach on retry"))
                         (assoc state 'talks 2)))))
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
             (def late (finish-cooks-assistant (dict 'page 0 'talks 0 'late #t)))
             (list (get report 'passed) (get report 'choices) (get report 'quest-points)
                   (get late 'passed))",
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(#t 2 1 #t)")
    );
}

#[test]
fn quest_wait_can_stop_before_a_transient_observation_disappears() {
    let mut world = World::default();
    agel_stdlib::install(&mut world, &Default::default()).unwrap();
    world
        .evaluate(
            r#"
      (module agel/osrs-mining-test)
      (module agel/osrs-game
        (export osrs-keepalive osrs-observe-bounded osrs-observe-ready osrs-wait-ticks
          osrs-inventory-count osrs-map-route osrs-ground-count osrs-take-ground-item)
        (def osrs-keepalive (fn (state) state))
        (def osrs-wait-ticks (fn (state _) (assoc state 'visible #f)))
        (def osrs-inventory-count (fn (state _container _item) (get state 'items)))
        (def osrs-map-route (fn (_) nil))
        (def osrs-ground-count (fn (state item x z level) (get state 'ground)))
        (def osrs-take-ground-item
          (fn (state item x z)
            (let ((attempts (+ 1 (get state 'attempts))))
              (if (= attempts 2)
                (if (= (get state 'ground) 1) (assoc (assoc state 'attempts attempts) 'items 1)
                  (signal 'test "attempted a pickup without ground evidence"))
                (assoc state 'attempts attempts)))))
        (def osrs-observe-ready
          (fn (state _ready _limit)
            (if (has-key? state 'attempts) state (assoc state 'items 1))))
        (def osrs-observe-bounded
          (fn (state ready _)
            (let ((visible (assoc state 'visible #t)))
              (if (ready visible) visible (assoc visible 'visible #f))))))
    "#,
        )
        .unwrap();
    world
        .evaluate(
            &include_str!("../../../examples/osrs/quest-test.agel").replace(
                "(export stage-cook-ingredients",
                "(export wait-live wait-item take-castle-pot stage-cook-ingredients",
            ),
        )
        .unwrap();
    let result = world
        .evaluate(
            "(import agel/osrs-quest-test)
      (def picked (take-castle-pot (dict 'items 0 'ground 1 'attempts 0) (fn (_) nil)))
      (def missing (with-handler (osrs/quest-test problem) #t
        (begin (take-castle-pot (dict 'items 0 'ground 0 'attempts 0) (fn (_) nil)) #f)))
      (list (get (wait-live (dict 'visible #f) (fn (state) (get state 'visible)) 1) 'visible)
            (get (wait-item (dict 'items 0) 1931) 'items) (get picked 'attempts) missing)",
        )
        .unwrap();
    assert_eq!(result.values.last(), Some(&Value::List(vec![Value::Bool(true), Value::Int(1), Value::Int(2), Value::Bool(true)])));
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
    let pause = world
        .evaluate(
            "(def cook-opened (decode original 93 '(162 0 56 2 0 103 128)))
             (def advertised (decode cook-opened 75 '(0 1 0 0 127 255 255 255 0 231 0 5 0 0 0 0)))
             (def unrelated (decode advertised 75 '(0 0 0 128 127 255 255 255 0 231 0 5 0 0 0 0)))
             (list (osrs-open-interface? cook-opened 231) (osrs-interface-opens cook-opened 231)
                   (osrs-pause-events cook-opened 231 5) (osrs-pause-events advertised 231 5)
                   (osrs-pause-events unrelated 231 5) (osrs-pause-events unrelated 231 6))",
        )
        .unwrap();
    assert_eq!(
        pause.values.last().map(Value::to_string).as_deref(),
        Some("(#t 1 0 1 1 0)")
    );
    let mining = include_str!("../../../examples/osrs/mining-test.agel").replace(
        "(export mine-first-copper",
        "(export fresh-tutor-menu? fresh-dialogue? mine-first-copper",
    );
    world.evaluate(&mining).unwrap();
    let tutor_trace = world
        .evaluate(
            "(import agel/osrs-mining-test)
             (def menu (decode original 93 '(162 0 56 2 0 91 128)))
             (def actionable (decode menu 75 '(0 1 0 0 132 0 1 0 0 219 0 1 0 0 0 0)))
             (def dismissed (decode actionable 45 '(0 162 2 56)))
             (list (fresh-tutor-menu? menu original) (fresh-tutor-menu? actionable original)
                   (fresh-tutor-menu? dismissed original)
                   (fresh-dialogue? cook-opened original 231 5)
                   (fresh-dialogue? advertised original 231 5)
                   (fresh-dialogue? unrelated original 231 6)
                   (fresh-dialogue? advertised advertised 231 5))",
        )
        .unwrap();
    assert_eq!(
        tutor_trace.values.last().map(Value::to_string).as_deref(),
        Some("(#f #t #f #f #t #f #f)")
    );
    let route = world
        .evaluate(
            r#"(def route-packet
                 (fn (state body)
                   (let ((first (isaac-bytes-next (get state 'in))))
                     (let ((second (isaac-bytes-next (get first 'state))))
                       (let ((a (+ 128 (get first 'value))) (b (+ 139 (get second 'value))))
                         (def raw (cons (- a (* 256 (/ a 256))) (cons (- b (* 256 (/ b 256))) body)))
                         (get (osrs-next-packet state) 'state))))))
              (def empty-route (route-packet original '(255 255 255 255)))
              (def walking (route-packet empty-route '(3 38 204 77)))
              (def stopped (route-packet walking '(255 255 255 255)))
              (def next-route (route-packet stopped '(3 39 76 77)))
              (list (osrs-map-route empty-route) (osrs-map-route walking)
                    (osrs-map-route stopped) (osrs-map-route next-route)
                    (osrs-route-cleared? stopped empty-route 3227 3149)
                    (osrs-route-cleared? stopped walking 3227 3149)
                    (osrs-route-cleared? stopped empty-route 3229 3149))"#,
        )
        .unwrap();
    assert_eq!(
        route.values.last().map(Value::to_string).as_deref(),
        Some("(nil {sequence 1 x 3227 z 3149 cleared #f} {sequence 1 x 3227 z 3149 cleared #t} {sequence 2 x 3229 z 3149 cleared #f} #t #f #f)")
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
      (def second-byte sent)
      (def skipped-direct (osrs-keepalive stopped))
      (osrs-service-start stopped)
      (def skipped-restart (run-step progress))
      (def now 6000000)
      (def sent-direct (osrs-keepalive stopped))
      (osrs-service-start sent-direct)
      (def skipped-after-direct (run-step progress))
      (list (= (get first-pulse 'bytes) first-byte)
        (= (get skipped 'bytes) nil)
        (= (get second-pulse 'bytes) second-byte)
        (= (get stopped 'out) (get direct 'out))
        (get stopped 'keepalives)
        (= skipped-direct stopped)
        (= (get skipped-restart 'bytes) nil)
        (= (get sent-direct 'last-keepalive-us) 6000000)
        (= (get skipped-after-direct 'bytes) nil))
    "#,
            &options,
        )
        .unwrap();
    assert_eq!(
        cadence.values.last().map(Value::to_string).as_deref(),
        Some("(#t #t #t #t 2 #t #t #t #t)")
    );
    let ready = world
        .evaluate_with(
            r#"
      (def opcode (+ 93 (get (isaac-bytes-next (get original 'in)) 'value)))
      (def raw (cons (- opcode (* 256 (/ opcode 256))) '(162 0 56 2 0 103 128)))
      (def maintained (assoc paced 'maintenance (assoc (get paced 'maintenance)
        'read-ready (fn () (if (= raw nil) #f #t)))))
      (def opened (osrs-observe-ready maintained (fn (state) (osrs-open-interface? state 231)) 1))
      (def drained (osrs-observe-ready opened (fn (state) #f) 1))
      (list (osrs-open-interface? opened 231) (= raw nil) (= drained opened)
            (= (osrs-observe-ready original (fn (state) #f) 1) original))"#,
            &options,
        )
        .unwrap();
    assert_eq!(
        ready.values.last().map(Value::to_string).as_deref(),
        Some("(#t #t #t #t)")
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
        (export osrs-keepalive osrs-observe-bounded osrs-observe-ready osrs-stat osrs-nearby-npcs osrs-attack
          osrs-walk osrs-open-interface? osrs-click-interface osrs-varp)
        (def rat-one (dict 'index 1 'type 2854))
        (def rat-two (dict 'index 2 'type 2854))
        (def osrs-keepalive (fn (state) state))
        (def osrs-observe-bounded (fn (state _ready _ticks) state))
        (def osrs-observe-ready
          (fn (state ready _limit)
            (if (get state 'late)
              (let ((next (assoc state 'stance (get state 'server-stance))))
                (if (ready next) next state))
              state)))
        (def osrs-stat
          (fn (state skill) (dict 'xp (get state skill) 'level 1)))
        (def osrs-nearby-npcs (fn (state) (get state 'npcs)))
        (def osrs-walk (fn (state x z) state))
        (def osrs-open-interface? (fn (state interface) #t))
        (def osrs-click-interface
          (fn (state interface component)
            (let ((clicked (assoc (assoc state 'clicks (cons component (get state 'clicks)))
                                  'server-stance (if (= component 10) 1 3))))
              (if (if (get state 'late) #t (get state 'clicks))
                (assoc (if (if (get state 'late) #f
                               (if (= component 10) #t (< 4 (count (get state 'clicks)))))
                         (assoc clicked 'stance (get clicked 'server-stance)) clicked)
                       'npcs (list rat-one rat-two))
                clicked))))
        (def osrs-varp (fn (state varp) (get state 'stance)))
        (def osrs-attack
          (fn (state index)
            (let ((next (assoc state 'attempts (cons index (get state 'attempts)))))
              (if (= (get state 'server-stance) nil)
                (assoc (assoc next 0 8) 3 1156)
                (if (= index 1) next
                  (assoc (assoc (assoc next 'npcs (list rat-one))
                  (if (= (get state 'server-stance) 1) 2 1) 8)
                  3 (+ (get state 3) 2))))))))
      (module agel/osrs-combat-test
        (export nearest-rat)
        (def nearest-rat (fn (_) (dict 'index 1 'type 2854))))
    "#,
        )
        .unwrap();
    let training = include_str!("../../../examples/osrs/combat-training.agel").replace(
        "(export train-rat-attack",
        "(export next-rat train-rat-attack",
    );
    world.evaluate(&training).unwrap();
    let result = world
        .evaluate(
            r#"
      (import agel/osrs-combat-training)
      (import agel/sequence)
      (def trained (train-rat-melee-result (dict 0 0 1 0 2 0 3 1154 'npcs (list (dict 'index 1 'type 2854))) 8 2 nil))
      (def report (get trained 'report))
      (def phases nil)
      (def late (train-rat-melee-with-progress (dict 0 0 1 0 2 0 3 1154 'late #t 'npcs (list (dict 'index 1 'type 2854))) 8 2 nil
        (fn (phase) (def phases (cons (get phase 'phase) phases)))))
      (list (get report 'passed) (get report 'defence-xp) (get report 'stance)
        (reverse (get (get trained 'state) 'clicks))
        (reverse (get (get trained 'state) 'attempts))
        (map (fn (phase) (get (car (get phase 'encounters)) 'target-index)) (get report 'phases))
        (get (get late 'report) 'stance)
        (reverse (get (get late 'state) 'clicks))
        (next-rat (dict 'npcs (list (dict 'index 2 'type 2854))) 1)
        (next-rat (dict 'npcs nil) 1) (reverse phases))
    "#,
        )
        .unwrap();
    assert_eq!(
        result.values.last().map(Value::to_string).as_deref(),
        Some("(#t 8 3 (10 18 10 18 10 18) (1 1 1 2 2) (1 2 2) 3 (10 18) 2 1 (training stance-requested training stance-requested training))")
    );
}
