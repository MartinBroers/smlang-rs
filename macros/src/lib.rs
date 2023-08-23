#![recursion_limit = "512"]

extern crate proc_macro;

mod codegen;
#[cfg(feature = "graphviz")]
mod diagramgen;
mod parser;
mod validation;

use syn::parse_macro_input;

// dot -Tsvg statemachine.gv -o statemachine.svg

#[proc_macro]
pub fn statemachine(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the syntax into structures
    let input = parse_macro_input!(input as parser::state_machine::StateMachine);

    // Validate syntax
    match parser::ParsedStateMachine::new(input) {
        // Generate code and hand the output tokens back to the compiler
        Ok(sm) => {
            #[cfg(feature = "graphviz")]
            {
                use std::hash::{Hash, Hasher};
                use std::io::Write;

                // Generate dot syntax for the statemachine.
                let diagram = diagramgen::generate_diagram(&sm);
                let diagram_name = if let Some(name) = &sm.name {
                    name.to_string()
                } else {
                    let mut diagram_hasher = std::collections::hash_map::DefaultHasher::new();
                    diagram.hash(&mut diagram_hasher);
                    format!("smlang{:010x}", diagram_hasher.finish())
                };

                // Start the 'dot' process.
                let mut process = std::process::Command::new("dot")
                    .args(&["-Tsvg", "-o", &format!("statemachine_{diagram_name}.svg")])
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                    .expect("Failed to execute 'dot'. Are you sure graphviz is installed?");

                // Write the dot syntax string to the 'dot' process stdin.
                process
                    .stdin
                    .as_mut()
                    .map(|s| s.write_all(diagram.as_bytes()));

                // Check the graphviz return status to see if it was successful.
                match process.wait() {
                    Ok(status) => {
                        if !status.success() {
                            panic!("'dot' failed to run. Are you sure graphviz is installed?");
                        }
                    }
                    Err(_) => panic!("'dot' failed to run. Are you sure graphviz is installed?"),
                }
            }

            // Validate the parsed state machine before generating code.
            if let Err(e) = validation::validate(&sm) {
                return e.to_compile_error().into();
            }

            codegen::generate_code(&sm).into()
        }
        Err(error) => error.to_compile_error().into(),
    }
}
