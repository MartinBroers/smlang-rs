//! An example of using state data to propagate events (See issue-17)

#![deny(missing_docs)]

use smlang::statemachine;

statemachine! {
    transitions: {
        *D0 +  ToD1 / to_d2  = D1,
        D1(Option<Events>) +  ToD2 / to_d3  = D2,
        D2(Option<Events>) +  ToD3 / to_d4  = D3,
        D3(Option<Events>) +  ToD4 / to_d5  = D4,
        D4(Option<Events>) +  ToD5  = D5,
    }
}

/// Context
pub struct Context;

impl StateMachineContext for Context {
    fn to_d2(&mut self) -> Option<Events> {
        Some(Events::ToD2)
    }

    fn to_d3(&mut self, _state_data: &Option<Events>) -> Option<Events> {
        Some(Events::ToD3)
    }

    fn to_d4(&mut self, _state_data: &Option<Events>) -> Option<Events> {
        Some(Events::ToD4)
    }

    fn to_d5(&mut self, _state_data: &Option<Events>) -> Option<Events> {
        Some(Events::ToD5)
    }

    fn on_exit_d0(&mut self) {
        println!("Exiting state D0");
    }
    fn on_exit_d1(&mut self) {
        println!("Exiting state D1");
    }
    fn on_entry_d4(&mut self) {
        println!("Entering state D4");
    }
}

// The macros does not derive Copy/Clone traits to the events, so we need to add them so that the
// event can be moved out of the state data
impl Copy for Events {}
impl Clone for Events {
    fn clone(&self) -> Self {
        *self
    }
}

fn main() {
    let mut sm = StateMachine::new(Context);

    // first event starts the dominos
    let mut event = Some(Events::ToD1);

    // use a while let loop to let the events propagate and the dominos fall
    while let Some(e) = event {
        let state = sm.process_event(e).unwrap();

        // use pattern matching to extract the event from any state with an action that fires one
        // good practice here NOT to use a wildcard to ensure you don't miss any states
        event = match state {
            States::D0 => None,
            States::D1(event) => *event,
            States::D2(event) => *event,
            States::D3(event) => *event,
            States::D4(event) => *event,
            States::D5 => None,
        };
    }

    // All the dominos fell!
    assert!(sm.state() == &States::D5);
}
