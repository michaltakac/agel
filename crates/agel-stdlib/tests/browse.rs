//! `agel/browse`: a browser written in Agel, and its agent. Pages come
//! from a loader the test supplies; the tree, the actions and the agent's
//! loop are checked on the host, where a fake judge answers.
use agel_core::{Budget, EvaluationOptions, Value, World};
use agel_model::JudgmentRequest;

const INDEX: &str = r#"<!DOCTYPE html>
<html><head><title>Widget &amp; Co</title><style>p { color: red }</style></head>
<body>
<!-- a comment with <tags> -->
<h1>Welcome   to
  Widget &amp; Co</h1>
<p>We sell <a href="/shop/blue.html">blue widgets</a> and <a href='red.html'>red widgets</a>.</p>
<ul><li>Fast shipping</li><li></li><li>Fair prices &lt;3</li></ul>
<script>document.write("<p>not a paragraph</p>")</script>
<form action="/shop/search.html"><input type="hidden" name="site" value="1">
<input name="q" value="" placeholder="search"><input type=submit value="Search"></form>
<p>Café — ünïcode ok</p>
</body></html>"#;

const BLUE: &str = r#"<html><head><title>Blue widget</title></head><body>
<h2>Blue widget</h2><p>Price: <b>$12</b> each.</p><a href="../index.html">Home</a>
<button>Buy</button></body></html>"#;

const SEARCH: &str = r#"<html><head><title>Results</title></head><body>
<h2>Results</h2><p>Nothing matched.</p><a href="/index.html">Home</a></body></html>"#;

/// Parsing a page byte by byte in Agel takes more fuel than a default
/// transaction has; the hosted runtime on the OS gives fifty million.
fn options() -> EvaluationOptions {
    EvaluationOptions {
        budget: Budget {
            fuel: 20_000_000,
            ..Budget::default()
        },
        ..EvaluationOptions::default()
    }
}

fn world() -> World {
    let mut world = World::new(0);
    let options = options();
    agel_stdlib::install(&mut world, &options).unwrap();
    run(&mut world, &format!(
            r#"(import agel/browse)
               (def pages (list (list "/index.html" {INDEX:?}) (list "/shop/blue.html" {BLUE:?}) (list "/shop/search.html" {SEARCH:?})))
               (def page-of (fn (pages url) (if (= pages nil) nil (if (= (car (car pages)) url) (car (cdr (car pages))) (page-of (cdr pages) url)))))
               (def loads nil)
               (browse-use (fn (url) (begin (def loads (cons url loads)) (page-of pages url))))"#
        ))
        .unwrap();
    world
}

fn run(world: &mut World, source: &str) -> Result<agel_core::Commit, agel_core::TransactionError> {
    world.evaluate_with(source, &options())
}

fn last(world: &mut World, source: &str) -> Value {
    run(world, source)
        .unwrap_or_else(|error| panic!("{source}: {error}"))
        .values
        .pop()
        .unwrap()
}

fn text(world: &mut World, source: &str) -> String {
    match last(world, source) {
        Value::String(text) => text,
        other => panic!("{source}: {other}"),
    }
}

#[test]
fn a_page_reads_as_a_numbered_tree() {
    let mut world = world();
    assert_eq!(
        last(&mut world, r#"(browse-open "/index.html")"#),
        Value::Int(11)
    );
    let tree = text(&mut world, "(browse-tree)");
    assert_eq!(
        tree,
        "page: Widget & Co (/index.html) form: /shop/search.html\n\
         1 heading Welcome to Widget & Co\n\
         2 text We sell\n\
         3 link blue widgets -> /shop/blue.html\n\
         4 text and\n\
         5 link red widgets -> red.html\n\
         6 text .\n\
         7 text Fast shipping\n\
         8 text Fair prices <3\n\
         9 field q = \"\"\n\
         10 button Search\n\
         11 text Café — ünïcode ok"
    );
    assert_eq!(
        text(&mut world, "(browse-state)"),
        "browse: Widget & Co | /index.html | 11 elements, 2 links, 1 fields | last: open /index.html"
    );
    assert_eq!(text(&mut world, "(car (cdr (browse-page)))"), "Widget & Co");
    assert_eq!(text(&mut world, "(car (browse-page))"), "/index.html");
}

#[test]
fn links_fields_submit_and_back_are_the_readers_actions() {
    let mut world = world();
    run(&mut world, r#"(browse-open "/index.html")"#).unwrap();
    // A relative link resolves against the page's directory.
    let error = run(&mut world, "(browse-link 5)").unwrap_err();
    assert!(error.to_string().contains("browse/not-found"), "{error}");
    assert_eq!(last(&mut world, "(browse-link 3)"), Value::Int(4));
    assert_eq!(
        text(&mut world, "(browse-tree)"),
        "page: Blue widget (/shop/blue.html)\n\
         1 heading Blue widget\n\
         2 text Price: $12 each.\n\
         3 link Home -> ../index.html\n\
         4 button Buy"
    );
    assert!(text(&mut world, "(browse-state)").contains("last: link 3 -> /shop/blue.html"));
    // The wrong kind of element is refused; back returns to the page before.
    let error = run(&mut world, "(browse-link 2)").unwrap_err();
    assert!(error.to_string().contains("browse/wrong-kind"), "{error}");
    assert_eq!(last(&mut world, "(browse-back)"), Value::Int(11));
    assert!(text(&mut world, "(browse-state)").contains("| last: back to /index.html"));
    let error = run(&mut world, "(browse-back)").unwrap_err();
    assert!(error.to_string().contains("browse/no-history"), "{error}");
    // Filling a field and submitting the form opens the action with the
    // fields as a query, noted, while the page itself is the static one.
    assert_eq!(
        last(&mut world, r#"(browse-fill 9 "blue")"#),
        Value::String("blue".into())
    );
    assert!(text(&mut world, "(browse-tree)").contains("9 field q = \"blue\""));
    assert_eq!(last(&mut world, "(browse-submit)"), Value::Int(3));
    assert!(text(&mut world, "(browse-state)")
        .contains("Results | /shop/search.html | 3 elements, 1 links, 0 fields | last: submit /shop/search.html?q=blue"));
    // The missing page is not in the log: a form that signals is a
    // transaction that rolled back, the loader's note with it.
    assert_eq!(
        last(&mut world, "loads").to_string(),
        r#"("/shop/search.html" "/index.html" "/shop/blue.html" "/index.html")"#
    );
    let error = run(&mut world, "(browse-submit)").unwrap_err();
    assert!(error.to_string().contains("browse/no-form"), "{error}");
}

#[test]
fn the_page_becomes_typed_questions_the_host_reads() {
    let mut world = world();
    run(&mut world, r#"(browse-open "/index.html") (browse-link 3)"#).unwrap();
    let request = text(
        &mut world,
        r#"(browse-questions "find the price of the \"blue\" widget")"#,
    );
    let parsed = JudgmentRequest::parse(&request).unwrap();
    let json = parsed.to_json("jev-latest");
    assert_eq!(
        json["state"]["task"],
        "find the price of the \"blue\" widget"
    );
    assert!(json["state"]["page"]
        .as_str()
        .unwrap()
        .starts_with("page: Blue widget (/shop/blue.html)\n1 heading Blue widget"));
    assert_eq!(json["state"]["fills_with"], "blue");
    let criteria = &json["questions"]["act"]["criteria"];
    assert_eq!(criteria["link-3"], "Home -> ../index.html");
    assert_eq!(criteria["back"], "go back to the previous page");
    assert!(criteria["done"].is_string());
    assert!(criteria.get("submit").is_none());
    assert_eq!(json["questions"]["done"]["type"], "noul");
    assert_eq!(
        text(&mut world, r#"(quoted-part "no quotes here")"#),
        "no quotes here"
    );
    // The answer line becomes an action: the choice, unless done is likely
    // or the choice is not confident.
    assert_eq!(
        text(
            &mut world,
            r#"(browse-decide "act choice 3 link-3 700 700 200 100 done noul 100")"#
        ),
        "link-3"
    );
    assert_eq!(
        text(
            &mut world,
            r#"(browse-decide "act choice 3 link-3 700 700 200 100 done noul 900")"#
        ),
        "done"
    );
    assert_eq!(
        text(
            &mut world,
            r#"(browse-decide "act choice 3 back 50 350 300 350 done noul 100")"#
        ),
        "done"
    );
}

#[test]
fn the_agent_drives_the_browser_with_a_judge_on_its_console() {
    let mut world = world();
    // A fake judge: a scripted answer per step, and a log of what it was
    // asked and what the browser said.
    run(&mut world,
            r#"(browse-open "/index.html")
               (def said nil)
               (def asked nil)
               (def replies (list
                 "act choice 4 fill-9 800 50 50 800 100 done noul 50"
                 "act choice 4 submit 900 30 30 40 900 done noul 100"
                 "act choice 3 link-3 600 600 200 200 done noul 200"
                 "act choice 4 done 900 20 20 60 900 done noul 950"))
               (def ask (fn (request) (begin (def asked (cons request asked)) (let ((line (car replies))) (begin (def replies (cdr replies)) line)))))
               (def say (fn (line) (def said (cons line said))))"#,
        )
        .unwrap();
    assert_eq!(
        last(
            &mut world,
            r#"(browse-drive "search for \"blue\" widgets, then find the price" 6 ask say)"#
        ),
        Value::Symbol("done".into())
    );
    let said = text(
        &mut world,
        r#"(def join (fn (lines acc) (if (= lines nil) acc (join (cdr lines) (text-concat (car lines) (text-concat "\n" acc)))))) (join said "")"#,
    );
    let lines: Vec<&str> = said.lines().collect();
    assert!(lines[0].starts_with("page: Widget & Co (/index.html) form: /shop/search.html"));
    assert!(lines.contains(
        &"browse: step 1 do fill-9 reason act choice 4 fill-9 800 50 50 800 100 done noul 50"
    ));
    assert!(lines.iter().any(|line| line.starts_with(
        "browse: Widget & Co | /index.html | 11 elements, 2 links, 1 fields | last: fill 9 q"
    )));
    assert!(lines.contains(
        &"browse: step 2 do submit reason act choice 4 submit 900 30 30 40 900 done noul 100"
    ));
    assert!(lines
        .iter()
        .any(|line| line.contains("last: submit /shop/search.html?q=blue")));
    assert!(lines.contains(
        &"browse: step 3 do link-3 reason act choice 3 link-3 600 600 200 200 done noul 200"
    ));
    assert!(lines
        .iter()
        .any(|line| line.contains("Widget & Co | /index.html | 11 elements")));
    assert!(lines.contains(
        &"browse: step 4 do done reason act choice 4 done 900 20 20 60 900 done noul 950"
    ));
    assert!(lines.contains(&"browse: done after 4 steps"));
    // The judge was asked with the page each time, and the filled field
    // was in the second page it saw.
    let Value::List(requests) = last(&mut world, "asked") else {
        panic!("asked is a list")
    };
    assert_eq!(requests.len(), 4);
    let second = text(&mut world, "(car (cdr (cdr asked)))");
    assert!(second.contains("9 field q = \\\"blue\\\""), "{second}");
    // A failing action is said, not fatal, and the loop goes on to its end.
    run(&mut world,
            r#"(def replies (list "act choice 4 link-9 900 20 20 60 900 done noul 100" "act choice 4 link-3 900 20 20 60 900 done noul 100"))"#,
        )
        .unwrap();
    assert_eq!(
        last(&mut world, r#"(browse-drive "anything" 2 ask say)"#),
        Value::Symbol("drove".into())
    );
    // Newest first: drove, the blue page's state, step 2, the index tree,
    // the index state after the failure, the failure, step 1, the tree.
    let after = text(&mut world, "(car (cdr said))");
    assert!(
        after.contains("browse: Blue widget | /shop/blue.html"),
        "{after}"
    );
    let failed = text(&mut world, "(car (cdr (cdr (cdr (cdr (cdr said))))))");
    assert_eq!(
        failed,
        "browse: link-9 failed: the element is not of that kind"
    );
}
