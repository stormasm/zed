Document as I remove crates from Zed and these are specific ones
that workspace depends upon. Yet everything will compile and run
over time so I can see a working version of the Welcome screen in the progression.

### Step 1 - branch wse0a

- Remove node_runtime crate from workspace Cargo.toml

- In order to do this prettier support needs to be removed from Project
- this involves removing -> crates/project/src/prettier_support.rs

### Step 2 - branch wse0b

- Remove features test-support from workspace and all of the associated crates which reference it

### Step 3 - branch wse0c

- Remove notifications.rs and shared_screen.rs
- Remove the crate dependencies call and client from workspace/Cargo.toml
