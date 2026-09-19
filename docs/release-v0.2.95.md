# Agel v0.2.95: a browser written in Agel, for agents

The fourth rung of the order of work, and the one the OS has been
missing: a browser. It is written in Agel — the `agel/browse` module of
the standard library — and runs on the OS as a process of the hosted
runtime, reading pages from the data region because the OS has no
network. It keeps a page as a numbered accessibility tree, acts as a
reader does, and carries its own agent, which asks a judge which of the
page's links and fields comes next for a task and whether the task is
done. The design is in [`browser.md`](browser.md).

## What is new

- **`agel/browse`**: `browse-parse` reads a reader's subset of HTML
  (headings, paragraphs and items, links, fields and buttons, the title,
  the form's action; comments, scripts and styles skipped; entities
  decoded; searches byte-wise so multi-byte text is safe) into elements
  `(N KIND TEXT LINK VALUE)`; `browse-tree` and `browse-state` print
  them; `browse-open`, `browse-link`, `browse-fill`, `browse-submit` and
  `browse-back` are the actions, with a loader named by `browse-use`.
- **The agent in the same module**: `browse-questions TASK` makes the
  page a typed request — a choice among `link-N`, `fill-N`, `submit`,
  `back` and `done`, and a `noul` on completion — `browse-decide` reads
  the answer line into an action, `browse-act` performs it (a field is
  filled with the task's quoted phrase; a System One model writes
  nothing), and `browse-drive TASK STEPS ASK SAY` runs the loop with the
  two words it is handed: `model-request` and `console-log` on the OS,
  anything in a test.
- **On the OS**: `:exec agel -- /data/browse.agel` over an installed
  site ([`examples/pages`](../examples/pages), three pages and a form;
  [`examples/browse-agent.agel`](../examples/browse-agent.agel) is the
  script). The process prints trees and steps on its console and reads
  its answers there.
- **`agel-play --scene browse --task TEXT [--pages DIR] [--agel PATH]`**:
  installs the runtime, the pages and a script with the task, starts the
  process, relays each of its `model-request N:` blocks to the provider
  with nothing added, types the answer back, and records the steps. The
  bridge now strips the desktop's prompt from a line a process prints
  after it.

## Transcript

Live, the key in the environment, the example site, six steps allowed.
The judge followed the link to the blue widget's page and, seeing the
price there, said the task was complete; 8 s wall clock with the boot:

```text
$ agel-play --scene browse --policy jev --steps 6 \
    --task "Find the price of the \"blue\" widget; the task is complete once a page shows it."
agel-play: 6 steps by the jev Agel program (browse-agent) into target/doom-runs/browse-live
agel-play: model reply 1: act choice 5 link-3 880 910 0 90 0 0 done noul 60
agel-play: step 1: link-3 []
agel-play: model reply 2: act choice 4 done 1000 0 0 0 1000 done noul 970
agel-play: step 2: done []
agel-play: done; the dataset is target/doom-runs/browse-live/steps.jsonl
```

The first request's choice had five options — the two links, the field,
submit and done — and the model put 910 thousandths on the blue link;
the second, on the blue page, had four — the two links, back and done —
and it put everything on done. What the process printed on the way, in
the OS test (`scripts/test-browse.sh`), where the harness is the judge
and fills the field, submits the form and reads the results first:

```text
page: Widget & Co (/data/index.html) form: /data/search.html
1 heading Welcome to Widget & Co
2 text We sell
3 link blue widgets -> /data/blue.html
4 text and
5 link red widgets -> /data/red.html
6 text , shipped the same day.
7 text Fast shipping
8 text Fair prices <3
9 field q = ""
10 button Search
model-request 1:
(judge (state ("task" "…") ("page" "…") ("fills_with" "blue") ("history" "open /data/index.html")) (choice "act" "The next action for the task on this page" ("link-3" "blue widgets -> /data/blue.html") ("link-5" "red widgets -> /data/red.html") ("fill-9" "fill the field q") ("submit" "submit the form with its fields as filled") ("done" "the task is complete, or nothing here advances it")) (noul "done" "Is the task complete, with what this page shows?" "complete" "not yet"))
model-request end
live-desktop> :model-reply 1 act choice 5 fill-9 800 30 30 800 70 70 done noul 50
browse: step 1 do fill-9 reason act choice 5 fill-9 800 30 30 800 70 70 done noul 50
browse: Widget & Co | /data/index.html | 10 elements, 2 links, 1 fields | last: fill 9 q
…
browse: step 2 do submit reason act choice 5 submit 900 20 20 40 900 20 done noul 100
browse: Search results | /data/search.html | 7 elements, 3 links, 0 fields | last: submit /data/search.html?q=blue
…
browse: step 3 do done reason act choice 4 done 700 100 100 100 700 done noul 900
browse: done after 3 steps
process agel exited with status 0
```

## Validation

- `crates/agel-stdlib/tests/browse.rs`: the tree element by element for
  a page with entities, a comment, a style, a script, nested links, an
  empty item, a hidden field and non-ASCII text; links, back, fill,
  submit and their refusals; the page as a request the provider parses;
  answer lines into actions; the loop with a scripted judge, a failing
  action said and survived.
- `scripts/test-browse.sh`: the process on the OS, this harness the
  judge, through fill, submit and done, exit 0.
- `scripts/test-play-bridge.sh`: the browse scene through the bridge
  with a stand-in provider, the page as the request's state, one step
  recorded; the DOOM, desktop and dataset episodes as before.
- By hand: the live transcript. The full local regression and CI.

## Not claimed

- A network, a window, HTML beyond the subset, free text from the model,
  pages beyond a transaction's budget, or any success rate: one task, one
  site, by hand. All of it is in [`browser.md`](browser.md).
