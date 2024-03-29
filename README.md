Document as I remove crates from Zed and these are specific ones
that workspace depends upon. Yet everything will compile and run
over time so I can see a working version of the Welcome screen in the progression.

### Step 1

- Remove node_runtime crate from workspace Cargo.toml

- In order to do this prettier support needs to be removed from Project
- this involves removing -> crates/project/src/prettier_support.rs
