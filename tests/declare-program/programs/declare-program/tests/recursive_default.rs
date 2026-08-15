use anchor_lang::prelude::*;

declare_program!(recursive_default);
use recursive_default::types::NodeVec;

#[test]
fn recursive_vec_types_can_default() {
    let node_vec = NodeVec::default();
    assert!(node_vec.children.is_empty());
}
