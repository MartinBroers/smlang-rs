// Move guards to return a Result

use crate::parser::data::Lifetimes;
use crate::parser::ParsedStateMachine;
use proc_macro2;
use proc_macro2::Span;
use quote::{format_ident, quote};
use std::vec::Vec;
use string_morph;
use syn::{punctuated::Punctuated, token::Paren, Type, TypeTuple};

pub fn generate_code(sm: &ParsedStateMachine) -> proc_macro2::TokenStream {
    // Get only the unique states
    let mut state_list: Vec<_> = sm.states.iter().map(|(_, value)| value).collect();
    state_list.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

    let state_list: Vec<_> = state_list
        .iter()
        .map(
            |value| match sm.state_data.data_types.get(&value.to_string()) {
                None => {
                    quote! {
                        #value
                    }
                }
                Some(t) => {
                    quote! {
                        #value(#t)
                    }
                }
            },
        )
        .collect();

    // Extract events
    let mut event_list: Vec<_> = sm.events.iter().map(|(_, value)| value).collect();
    event_list.sort_by(|a, b| a.to_string().cmp(&b.to_string()));

    // Extract events
    let event_list: Vec<_> = event_list
        .iter()
        .map(
            |value| match sm.event_data.data_types.get(&value.to_string()) {
                None => {
                    quote! {
                        #value
                    }
                }
                Some(t) => {
                    quote! {
                        #value(#t)
                    }
                }
            },
        )
        .collect();

    let transitions = &sm.states_events_mapping;

    let in_states: Vec<_> = transitions
        .iter()
        .map(|(name, _)| {
            let state_name = sm.states.get(name).unwrap();

            match sm.state_data.data_types.get(name) {
                None => {
                    quote! {
                        #state_name
                    }
                }
                Some(_) => {
                    quote! {
                        #state_name(ref state_data)
                    }
                }
            }
        })
        .collect();

    let events: Vec<Vec<_>> = transitions
        .iter()
        .map(|(_, value)| {
            value
                .iter()
                .map(|(name, value)| {
                    let value = &value.event;

                    match sm.event_data.data_types.get(name) {
                        None => {
                            quote! {
                                #value
                            }
                        }
                        Some(_) => {
                            quote! {
                                #value(ref mut event_data)
                            }
                        }
                    }
                })
                .collect()
        })
        .collect();

    // println!("sm: {:#?}", sm);
    // println!("in_states: {:#?}", in_states);
    // println!("events: {:#?}", events);
    // println!("transitions: {:#?}", transitions);

    // Map guards, actions and output states into code blocks
    let guards: Vec<Vec<_>> = transitions
        .iter()
        .map(|(_, value)| value.iter().map(|(_, value)| &value.guard).collect())
        .collect();

    let actions: Vec<Vec<_>> = transitions
        .iter()
        .map(|(_, value)| value.iter().map(|(_, value)| &value.action).collect())
        .collect();

    let guard_action_parameters: Vec<Vec<_>> = transitions
        .iter()
        .map(|(name, value)| {
            let state_name = &sm.states.get(name).unwrap().to_string();

            value
                .iter()
                .map(|(name, _)| {
                    // let event_name = &value.event;

                    match (
                        sm.state_data.data_types.get(state_name),
                        sm.event_data.data_types.get(name),
                    ) {
                        (None, None) => {
                            quote! {}
                        }
                        (Some(_), None) => {
                            quote! {
                                state_data
                            }
                        }
                        (None, Some(_)) => {
                            quote! {
                                event_data
                            }
                        }
                        (Some(_), Some(_)) => {
                            quote! {
                                state_data, event_data
                            }
                        }
                    }
                })
                .collect()
        })
        .collect();

    let out_states: Vec<Vec<_>> = transitions
        .iter()
        .map(|(_, value)| {
            value
                .iter()
                .map(|(_, value)| {
                    let out_state = &value.out_state;

                    match sm.state_data.data_types.get(&out_state.to_string()) {
                        None => {
                            quote! {
                                #out_state
                            }
                        }
                        Some(_) => {
                            quote! {
                                #out_state(_data)
                            }
                        }
                    }
                })
                .collect()
        })
        .collect();

    let temporary_context = match &sm.temporary_context_type {
        Some(tct) => {
            quote! { temporary_context: #tct, }
        }
        None => {
            quote! {}
        }
    };

    // Keep track of already added actions not to duplicate definitions
    let mut action_set: Vec<syn::Ident> = Vec::new();
    let mut guard_set: Vec<syn::Ident> = Vec::new();

    let mut guard_list = proc_macro2::TokenStream::new();
    let mut action_list = proc_macro2::TokenStream::new();
    let mut entry_list = proc_macro2::TokenStream::new();
    for (state, value) in transitions.iter() {
        // create the state data token stream
        let state_data = match sm.state_data.data_types.get(state) {
            Some(st) => quote! { state_data: &#st, },
            None => quote! {},
        };

        let entry_ident = format_ident!("on_entry_{}", string_morph::to_snake_case(state));
        entry_list.extend(quote! {
            #[allow(missing_docs)]
            fn #entry_ident(&mut self){}
        });
        let exit_ident = format_ident!("on_exit_{}", string_morph::to_snake_case(state));
        entry_list.extend(quote! {
           #[allow(missing_docs)]
           fn #exit_ident(&mut self){}
        });

        value.iter().for_each(|(event, value)| {


            // get output state lifetimes
            let state_lifetimes = if let Some(lifetimes) = sm.state_data.lifetimes.get(&value.out_state.to_string()) {
                lifetimes.clone()
            } else {
                Lifetimes::new()
            };

            // get the event lifetimes
            let mut lifetimes = if let Some(lifetimes) = sm.event_data.lifetimes.get(event) {
                lifetimes.clone()
            } else {
                Lifetimes::new()
            };

            // combine the state data and event data lifetimes
            lifetimes.append(&mut state_lifetimes.clone());

            // Create the guard traits for user implementation
            if let Some(guard) = &value.guard {
                let guard_with_lifetimes = if let Some(lifetimes) = sm.event_data.lifetimes.get(event) {
                    let lifetimes = &lifetimes;
                    quote! {
                        #guard<#(#lifetimes),*>
                    }
                } else {
                    quote! {
                        #guard
                    }
                };

                let event_data = match sm.event_data.data_types.get(event) {
                    Some(et) => match et {
                        Type::Reference(_) => {
                            quote! { event_data: #et }
                        }
                        _ => {
                            quote! { event_data: &#et }
                        }
                    },
                    None => {
                        quote! {}
                    }
                };

                let guard_error = if sm.custom_guard_error {
                    quote! { Self::GuardError }

                } else {
                    quote! { () }
                };

                // Only add the guard if it hasn't been added before
                if guard_set.iter().find(|a| a == &guard).is_none() {
                    guard_set.push(guard.clone());
                    guard_list.extend(quote! {
                        #[allow(missing_docs)]
                        fn #guard_with_lifetimes(&mut self, #temporary_context #state_data #event_data) -> Result<(), #guard_error>;
                    });
                }
            }

            // Create the action traits for user implementation
            if let Some(action) = &value.action {
                let return_type = if let Some(output_data) =
                    sm.state_data.data_types.get(&value.out_state.to_string())
                {
                    output_data.clone()
                } else {
                    // Empty return type
                    Type::Tuple(TypeTuple {
                        paren_token: Paren {
                            span: Span::call_site(),
                        },
                        elems: Punctuated::new(),
                    })
                };

                let action_with_lifetimes = if lifetimes.is_empty() {
                    quote! {
                        #action
                    }
                } else {
                    quote! {
                        #action<#(#lifetimes),*>
                    }
                };

                let state_data = match sm.state_data.data_types.get(state) {
                    Some(st) => {
                        quote! { state_data: &#st, }
                    }
                    None => {
                        quote! {}
                    }
                };
                let event_data = match sm.event_data.data_types.get(event) {
                    Some(et) => match et {
                        Type::Reference(_) => {
                            quote! { event_data: #et }
                        }
                        _ => {
                            quote! { event_data: &#et }
                        }
                    },
                    None => {
                        quote! {}
                    }
                };

                // Only add the action if it hasn't been added before
                if action_set.iter().find(|a| a == &action).is_none() {
                    action_set.push(action.clone());
                    action_list.extend(quote! {
                        #[allow(missing_docs)]
                        fn #action_with_lifetimes(&mut self, #temporary_context #state_data #event_data) -> #return_type;
                    });
                }
            }
        })
    }

    let temporary_context_call = match &sm.temporary_context_type {
        Some(_) => {
            quote! { temporary_context, }
        }
        None => {
            quote! {}
        }
    };

    // Create the code blocks inside the switch cases
    let code_blocks: Vec<Vec<_>> = guards
        .iter()
        .zip(
            actions
                .iter()
                .zip(in_states.iter()
                .zip(out_states.iter().zip(guard_action_parameters.iter()))),
        )
        .map(
            |(guards, (actions, (in_state, (out_states, guard_action_parameters))))| {
                guards
                    .iter()
                    .zip(
                        actions
                            .iter()
                            .zip(out_states.iter().zip(guard_action_parameters.iter())),
                    )
                    .map(|(guard, (action,  (out_state, g_a_param )))| {
                        let out_state_string = &out_state.to_string()[0..out_state.to_string().find('(').unwrap_or_else(|| out_state.to_string().len())];
                        let entry_ident = format_ident!("on_entry_{}",string_morph::to_snake_case(out_state_string ));
                        let in_state_string = &in_state.to_string()[0..in_state.to_string().find('(').unwrap_or_else(|| in_state.to_string().len())];
                        let exit_ident = format_ident!("on_exit_{}",string_morph::to_snake_case(in_state_string));
                        if let Some(g) = guard {
                            if let Some(a) = action {
                                quote! {
                                    self.context.#g(#temporary_context_call #g_a_param).map_err(Error::GuardFailed)?;
                                    let _data = self.context.#a(#temporary_context_call #g_a_param);
                                    self.context_mut().#exit_ident();
                                    self.context_mut().#entry_ident();
                                    self.state = States::#out_state;
                                }
                            } else {
                                quote! {
                                    self.context.#g(#temporary_context_call #g_a_param).map_err(Error::GuardFailed)?;
                                    self.context_mut().#exit_ident();
                                    self.context_mut().#entry_ident();
                                    self.state = States::#out_state;
                                }
                            }
                        } else {
                            if let Some(a) = action {
                                quote! {
                                    let _data = self.context.#a(#temporary_context_call #g_a_param);
                                    self.context_mut().#exit_ident();
                                    self.context_mut().#entry_ident();
                                    self.state = States::#out_state;
                                }
                            } else {
                                quote! {
                                    self.context_mut().#exit_ident();
                                    self.context_mut().#entry_ident();
                                    self.state = States::#out_state;
                                }
                            }
                        }
                    })
                    .collect()
            },
        )
        .collect();

    let starting_state = &sm.starting_state;

    // create a token stream for creating a new machine.  If the starting state contains data, then
    // add a second argument to pass this initial data
    let starting_state_name = starting_state.to_string();
    let new_sm_code = match sm.state_data.data_types.get(&starting_state_name) {
        Some(st) => quote! {
            pub fn new(context: T, state_data: #st ) -> Self {
                StateMachine {
                    state: States::#starting_state (state_data),
                    context
                }
            }
        },
        None => quote! {
            pub fn new(context: T ) -> Self {
                StateMachine {
                    state: States::#starting_state,
                    context
                }
            }
        },
    };

    // create token-streams for state data lifetimes
    let state_lifetimes_code = if sm.state_data.lifetimes.is_empty() {
        quote! {}
    } else {
        let state_lifetimes = &sm.state_data.all_lifetimes;
        quote! {#(#state_lifetimes),* ,}
    };

    // create token-streams for event data lifetimes
    let event_lifetimes_code = if sm.event_data.lifetimes.is_empty() {
        quote! {}
    } else {
        let event_lifetimes = &sm.event_data.all_lifetimes;
        quote! {#(#event_lifetimes),* ,}
    };

    let guard_error = if sm.custom_guard_error {
        quote! {
            /// The error type returned by guard functions.
            type GuardError: core::fmt::Debug;
        }
    } else {
        quote! {}
    };

    let error_type = if sm.custom_guard_error {
        quote! {
            Error<<T as StateMachineContext>::GuardError>
        }
    } else {
        quote! {Error}
    };

    // Build the states and events output
    quote! {
        /// This trait outlines the guards and actions that need to be implemented for the state
        /// machine.
        pub trait StateMachineContext {
            #guard_error
            #guard_list
            #action_list
            #entry_list
        }

        /// List of auto-generated states.
        #[allow(missing_docs)]
        #[derive(Debug)]
        pub enum States <#state_lifetimes_code> { #(#state_list),* }

        /// Manually define PartialEq for States based on variant only to address issue-#21
        impl<#state_lifetimes_code> PartialEq for States <#state_lifetimes_code> {
            fn eq(&self, other: &Self) -> bool {
                use core::mem::discriminant;
                discriminant(self) == discriminant(other)
            }
        }

        /// List of auto-generated events.
        #[allow(missing_docs)]
        #[derive(Debug)]
        pub enum Events <#event_lifetimes_code> { #(#event_list),* }

        /// Manually define PartialEq for Events based on variant only to address issue-#21
        impl<#event_lifetimes_code> PartialEq for Events <#event_lifetimes_code> {
            fn eq(&self, other: &Self) -> bool {
                use core::mem::discriminant;
                discriminant(self) == discriminant(other)
            }
        }

        /// List of possible errors
        #[derive(Debug)]
        pub enum Error<T=()> {
            /// When an event is processed which should not come in the current state.
            InvalidEvent,
            /// When an event is processed whose guard did not return `true`.
            GuardFailed(T),
        }

        /// State machine structure definition.
        pub struct StateMachine<#state_lifetimes_code T: StateMachineContext> {
            state: States <#state_lifetimes_code>,
            context: T
        }

        impl<#state_lifetimes_code T: StateMachineContext> StateMachine<#state_lifetimes_code T> {
            /// Creates a new state machine with the specified starting state.
            #[inline(always)]
            #new_sm_code

            /// Creates a new state machine with an initial state.
            #[inline(always)]
            pub fn new_with_state(context: T, initial_state: States <#state_lifetimes_code>) -> Self {
                StateMachine {
                    state: initial_state,
                    context
                }
            }

            /// Returns the current state.
            #[inline(always)]
            pub fn state(&self) -> &States {
                &self.state
            }

            /// Returns the current context.
            #[inline(always)]
            pub fn context(&self) -> &T {
                &self.context
            }

            /// Returns the current context as a mutable reference.
            #[inline(always)]
            pub fn context_mut(&mut self) -> &mut T {
                &mut self.context
            }

            /// Process an event.
            ///
            /// It will return `Ok(&NextState)` if the transition was successful, or `Err(Error)`
            /// if there was an error in the transition.
            pub fn process_event(&mut self, #temporary_context mut event: Events) -> Result<&States, #error_type> {
                match self.state {
                    #(States::#in_states => match event {
                        #(Events::#events => {
                            #code_blocks

                            Ok(&self.state)
                        }),*
                        _ => Err(Error::InvalidEvent),
                    }),*
                    _ => Err(Error::InvalidEvent),
                }
            }
        }
    }
}
