pub mod data;
pub mod event;
pub mod input_state;
pub mod state_machine;
pub mod transition;

use data::{DataDefinitions, Lifetimes};
use event::EventMapping;
use state_machine::StateMachine;

use input_state::InputState;
use proc_macro2::Span;

use std::collections::HashMap;
use syn::{parse, spanned::Spanned, GenericArgument, Ident, PathArguments, Type};
use transition::StateTransition;

pub type TransitionMap = HashMap<String, HashMap<String, EventMapping>>;

#[derive(Debug)]
pub struct ParsedStateMachine {
    pub temporary_context_type: Option<Type>,
    pub guard_error: Option<Type>,
    pub states: HashMap<String, Ident>,
    pub starting_state: Ident,
    pub state_data: DataDefinitions,
    pub events: HashMap<String, Ident>,
    pub event_data: DataDefinitions,
    pub states_events_mapping: HashMap<String, HashMap<String, EventMapping>>,
}

// helper function for extracting a vector of lifetimes from a Type
fn get_lifetimes(data_type: &Type) -> Result<Lifetimes, parse::Error> {
    let mut lifetimes = Lifetimes::new();
    match data_type {
        Type::Reference(tr) => {
            if let Some(lifetime) = &tr.lifetime {
                lifetimes.push(lifetime.clone());
            } else {
                return Err(parse::Error::new(
                    data_type.span(),
                    "This event's data lifetime is not defined, consider adding a lifetime.",
                ));
            }
            Ok(lifetimes)
        }
        Type::Path(tp) => {
            let punct = &tp.path.segments;
            for p in punct.iter() {
                if let PathArguments::AngleBracketed(abga) = &p.arguments {
                    for arg in &abga.args {
                        if let GenericArgument::Lifetime(lifetime) = &arg {
                            lifetimes.push(lifetime.clone());
                        }
                    }
                }
            }
            Ok(lifetimes)
        }
        _ => Ok(lifetimes),
    }
}

// helper function for adding a new data type to a data descriptions struct
fn add_new_data_type(
    key: String,
    data_type: Type,
    definitions: &mut DataDefinitions,
) -> Result<(), parse::Error> {
    // retrieve any lifetimes used in this data-type
    let mut lifetimes = get_lifetimes(&data_type)?;

    // add the data to the collection
    definitions.data_types.insert(key.clone(), data_type);

    // if any new lifetimes were used in the type definition, we add those as well
    if !lifetimes.is_empty() {
        definitions.lifetimes.insert(key, lifetimes.clone());
        definitions.all_lifetimes.append(&mut lifetimes);
    }
    Ok(())
}

// helper function for collecting data types and adding them to a data descriptions struct
fn collect_data_type(
    key: String,
    data_type: Option<Type>,
    definitions: &mut DataDefinitions,
) -> Result<(), parse::Error> {
    // check to see if there was every a previous data-type associated with this transition
    let prev = definitions.data_types.get(&key);

    // if there was a previous data definition for this key, may sure it is consistent
    if let Some(prev) = prev {
        if let Some(ref data_type) = data_type {
            if prev != &data_type.clone() {
                return Err(parse::Error::new(
                    data_type.span(),
                    "This event's type does not match its previous definition.",
                ));
            }
        } else {
            return Err(parse::Error::new(
                data_type.span(),
                "This event's type does not match its previous definition.",
            ));
        }
    }

    if let Some(data_type) = data_type {
        add_new_data_type(key, data_type, definitions)?;
    }
    Ok(())
}

// helper function for adding a transition to a transition event map
fn add_transition(
    transition: &StateTransition,
    transition_map: &mut TransitionMap,
    state_data: &DataDefinitions,
) -> Result<(), parse::Error> {
    let p = transition_map
        .get_mut(&transition.in_state.ident.to_string())
        .unwrap();

    if !p.contains_key(&transition.event.to_string()) {
        let mapping = EventMapping {
            event: transition.event.clone(),
            guard: transition.guard.clone(),
            action: transition.action.clone(),
            out_state: transition.out_state.clone(),
        };

        p.insert(transition.event.to_string(), mapping);
    } else {
        return Err(parse::Error::new(
            transition.in_state.ident.span(),
            "State and event combination specified multiple times, remove duplicates.",
        ));
    }

    // Check for actions when states have data a
    if let Some(_) = state_data.data_types.get(&transition.out_state.to_string()) {
        // This transition goes to a state that has data associated, check so it has an
        // action

        if transition.action.is_none() {
            return Err(parse::Error::new(
                transition.out_state.span(),
                "This state has data associated, but not action is define here to provide it.",
            ));
        }
    }
    Ok(())
}

impl ParsedStateMachine {
    pub fn new(sm: StateMachine) -> parse::Result<Self> {
        // Check the initial state definition
        let num_start: usize = sm
            .transitions
            .iter()
            .map(|sm| if sm.in_state.start { 1 } else { 0 })
            .sum();

        if num_start == 0 {
            return Err(parse::Error::new(
                Span::call_site(),
                "No starting state defined, indicate the starting state with a *.",
            ));
        } else if num_start > 1 {
            return Err(parse::Error::new(
                Span::call_site(),
                "More than one starting state defined (indicated with *), remove duplicates.",
            ));
        }

        // Extract the starting state
        let starting_state = sm
            .transitions
            .iter()
            .find(|sm| sm.in_state.start)
            .unwrap()
            .in_state
            .ident
            .clone();

        let mut states = HashMap::new();
        let mut state_data = DataDefinitions::new();
        let mut events = HashMap::new();
        let mut event_data = DataDefinitions::new();
        let mut states_events_mapping = TransitionMap::new();

        for transition in sm.transitions.iter() {
            // Collect states
            let in_state_name = transition.in_state.ident.to_string();
            let out_state_name = transition.out_state.to_string();
            if !transition.in_state.wildcard {
                states.insert(in_state_name.clone(), transition.in_state.ident.clone());
                collect_data_type(
                    in_state_name.clone(),
                    transition.in_state.data_type.clone(),
                    &mut state_data,
                )?;
            }
            states.insert(out_state_name.clone(), transition.out_state.clone());
            collect_data_type(
                out_state_name.clone(),
                transition.out_state_data_type.clone(),
                &mut state_data,
            )?;

            // Collect events
            let event_name = transition.event.to_string();
            events.insert(event_name.clone(), transition.event.clone());
            collect_data_type(
                event_name.clone(),
                transition.event_data_type.clone(),
                &mut event_data,
            )?;

            // add input and output states to the mapping HashMap
            if !transition.in_state.wildcard {
                states_events_mapping.insert(transition.in_state.ident.to_string(), HashMap::new());
            }
            states_events_mapping.insert(transition.out_state.to_string(), HashMap::new());
        }

        // Remove duplicate lifetimes
        state_data.all_lifetimes.dedup();
        event_data.all_lifetimes.dedup();

        for transition in sm.transitions.iter() {
            // if input state is a wildcard, we need to add this transition for all states
            if transition.in_state.wildcard {
                for (name, in_state) in &states {
                    // create a new input state from wildcard
                    let in_state = InputState {
                        start: false,
                        wildcard: false,
                        ident: in_state.clone(),
                        data_type: state_data.data_types.get(name).cloned(),
                    };

                    // create the transition
                    let wildcard_transition = StateTransition {
                        in_state,
                        event: transition.event.clone(),
                        event_data_type: transition.event_data_type.clone(),
                        guard: transition.guard.clone(),
                        action: transition.action.clone(),
                        out_state: transition.out_state.clone(),
                        out_state_data_type: transition.out_state_data_type.clone(),
                    };

                    // add the wildcard transition to the transition map
                    // TODO:  Need to work on the span of this error, as it is being caused by the wildcard
                    // but won't show up at that line
                    add_transition(
                        &wildcard_transition,
                        &mut states_events_mapping,
                        &state_data,
                    )?;
                }
            } else {
                add_transition(transition, &mut states_events_mapping, &state_data)?;
            }
        }

        Ok(ParsedStateMachine {
            temporary_context_type: sm.temporary_context_type,
            guard_error: sm.guard_error,
            states,
            starting_state,
            state_data,
            events,
            event_data,
            states_events_mapping,
        })
    }
}
