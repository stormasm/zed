branch _wsd0b_ basically takes you down to 2 errors where item.rs is
the only issue to resolve...

so what I have done is dramatically pruned back workspace.rs and then
gone from there to this point where we need to now address item.rs

we started with a clean workspace crate and removed workspace.rs and
then built it back up to the state its in on this branch.

and got to a place where there are two errors with item.rs

now we need to leverage this code and knowledge to move forward.

the goal is to get a clean compilation of workspace so we can see
how it works with a very reduced workspace.rs and item.rs
