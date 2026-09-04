use agel_interaction::{ForegroundEvent, Input, Intent, InteractionHub, PresenceAuthority};

fn main() {
    let authority = PresenceAuthority::new();
    let mut hub = InteractionHub::new(8, &authority).unwrap();
    let id = hub
        .submit(Input::voice(
            "What changed in the candidate world?",
            Intent::Observe,
        ))
        .unwrap();

    println!("foreground: {:?}", hub.next_foreground().unwrap());
    let work = hub.next_background().unwrap();
    println!("background: #{} {:?}", work.id, work.input.content);
    hub.complete(id, "Candidate B changes only the scheduler library.")
        .unwrap();
    if let Some(ForegroundEvent::Completed { response, .. }) = hub.next_foreground() {
        println!("foreground: {response}");
    }

    let denied = hub.submit(Input::voice("promote candidate B", Intent::Authorize));
    println!("unverified voice authority: {}", denied.unwrap_err());
}
