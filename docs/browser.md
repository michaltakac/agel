# A browser written in Agel, for agents

Since v0.2.95 there is a browser on the OS Agel boots, and it is Agel:
the `agel/browse` module of the standard library, run by the hosted
runtime as a process, reading pages from the data region because the OS
has no network, keeping each page as a numbered accessibility tree, and
acting as a reader does — follow a link, fill a field, submit the form,
go back. The agent is in the same module: the tree becomes typed
questions to a judge, the answer line becomes an action, and the loop
ends when the judge says the task is done. It is the fourth rung of the
order of work in [`system-one.md`](system-one.md), after the language,
the desktop and the game, and it is the smallest honest one: no window,
no network, a subset of HTML, and a judge that never writes.

## The pieces

**The parser.** `browse-parse HTML URL` tokenizes the page byte by byte
(tags, closes, text; comments, `script`, `style` and `select` skipped)
and builds elements from what carries meaning for a reader: `h1`–`h3` as
headings; `p`, `li`, `td`, `th`, `label`, `dd`, `dt` as text; `a` as
links, interrupting the text around them; `input` as fields (or a button
when `type=submit`, nothing when hidden), `textarea` as a field, `button`
as a button; `title` as the page's; the first `form`'s `action`. Entities
`&amp; &lt; &gt; &quot; &#39; &nbsp;` are decoded, whitespace collapsed,
and searches compare bytes so a multi-byte character is never sliced.
Everything else is ignored, and text outside any container is an element
of its own.

**The tree.** `browse-tree` prints the page as lines:

```text
page: Widget & Co (/data/index.html) form: /data/search.html
1 heading Welcome to Widget & Co
2 text We sell
3 link blue widgets -> /data/blue.html
…
9 field q = ""
10 button Search
```

and `browse-state` as one: `browse: TITLE | URL | N elements, L links,
F fields | last: ACTION`. `browse-page` is the same as data.

**The actions.** `browse-use LOADER` names the function that turns a
path into a page's text (`file-read` on the OS). `browse-open PATH`
resolves the path against the current page's directory (`?query` and
`#fragment` dropped) and loads it; `browse-link N` follows link N;
`browse-fill N TEXT` sets field N; `browse-submit` opens the form's
action with the fields as `name=value&…` noted beside the page, since a
page in the data region cannot process a query; `browse-back` returns.
A wrong element, a missing page, a page without a form or a history
without a past are signals, and a failing form is a transaction that
rolls back.

**The agent.** `browse-questions TASK` turns the page into a request:
the state is the task, the tree, the phrase fields are filled with, and
the last action; the questions are a `choice` among `link-N` (with the
link's text and target), `fill-N` (with the field's name), `submit` when
there is a form, `back` when there is a past, and `done`, and a `noul`
on whether the task is complete with what the page shows.
`browse-decide LINE` reads the answer: `done` above 500 thousandths, or
a choice under 100 thousandths of confidence, ends the run; otherwise the
chosen name is the action. `browse-act ACTION TASK` performs it, filling
a field with `quoted-part TASK`, the task's first phrase in double
quotes, because a System One model answers typed questions and writes
nothing. `browse-drive TASK STEPS ASK SAY` is the loop: it says the tree,
asks, says `browse: step N do ACTION reason LINE`, acts (a failure is
said, not fatal), says the state, and stops on `done`
(`browse: done after N steps`) or after its steps (`browse: drove N
steps`). On the OS, `ASK` is `model-request` — a block on the process's
own console, answered by whoever sits at the desktop or by the host
bridge — and `SAY` is `console-log`.

**The process.** `:exec agel -- /data/browse.agel` runs a script such
as [`examples/browse-agent.agel`](../examples/browse-agent.agel) in the
hosted runtime with its fifty-million-step budget. The site is whatever
was installed into the data region ([`examples/pages`](../examples/pages)
is three pages and a form); the process prints trees and steps on its
console and reads its answers there.

**The bridge.** `agel-play --scene browse --task TEXT [--pages DIR]
[--agel PATH]` installs the hosted runtime, the pages and a script with
the task and the step count written into it, starts the process, relays
each `model-request N:` block to the provider with nothing added — the
request already carries the task and the page — and types the answer
line back to the process. Each step is recorded in `steps.jsonl` as the
others are.

## What is proven, and where

- `crates/agel-stdlib/tests/browse.rs`, on the host with a loader over
  three pages: the tree of a page with entities, a comment, a style, a
  script, nested links, an empty item, a hidden field and non-ASCII text,
  element by element; the state line; links resolving relative to the
  page and a missing page refused; the wrong kind of element refused;
  back, and back with no past; fill, submit as a query, and no form; the
  page as a request the provider parses, with the options and the fills;
  answer lines into actions; and the whole loop with a scripted judge —
  fill, submit, link, done — with a failing action said and survived.
- `scripts/test-browse.sh`: on the OS, the hosted runtime running the
  agent's script over the installed site, this harness the judge: the
  first request carries the task, the tree and the options; the field is
  filled with the task's phrase and the tree shows it; the form is
  submitted and the results page read; the judge's done ends the run and
  the process exits 0.
- `scripts/test-play-bridge.sh`: the same through the bridge with a
  stand-in provider that answers done at once — the runtime, pages and
  script installed, the block relayed with the page as its state, the
  answer typed to the process, one step recorded.
- By hand with `TYPESAFEAI_API_KEY`: the live judge over the example site
  (transcript in [v0.2.95](release-v0.2.95.md)).

## Not claimed

- A network. Pages come from the data region, installed with the image;
  a fetch through the host bridge is the obvious next step and is not
  here. Nothing on the OS opens a socket.
- A window. The browser shows nothing but its console; the desktop's
  windows and the driving loop of v0.2.93 are not involved.
- HTML beyond the subset above: no tables as tables, no nested forms, no
  `select`, no CSS, no scripts, no images, no attributes but the ones
  named. A page that relies on any of them reads wrongly.
- Free text from the model. A field is filled with the task's quoted
  phrase; the model chooses which field and when.
- Pages larger than a transaction's budget can parse: parsing is byte
  by byte in Agel and takes about a hundred steps a byte; the hosted
  runtime's budget is fifty million steps.
- Any success rate: one task, one site, by hand.
